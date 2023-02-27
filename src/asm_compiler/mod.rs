use std::collections::{BTreeMap, HashMap};

use crate::parser::asm_ast::*;
use crate::parser::ast::*;
use crate::parser::{self, ParseError};

pub fn compile<'a>(file_name: Option<&str>, input: &'a str) -> Result<PILFile, ParseError<'a>> {
    parser::parse_asm(file_name, input).map(|ast| ASMPILConverter::new().convert(ast))
}

#[derive(Default)]
struct ASMPILConverter {
    pil: Vec<Statement>,
    pc_name: Option<String>,
    default_assignment: Option<String>,
    registers: BTreeMap<String, Register>,
    instructions: BTreeMap<String, Instruction>,
    code_lines: Vec<CodeLine>,
    /// Pairs of columns that are used in the connecting plookup
    line_lookup: Vec<(String, String)>,
    /// Names of fixed columns that contain the program.
    program_constant_names: Vec<String>,
}

impl ASMPILConverter {
    fn new() -> Self {
        Default::default()
    }

    fn convert(&mut self, input: ASMFile) -> PILFile {
        for statement in &input.0 {
            match statement {
                ASMStatement::RegisterDeclaration(start, name, flags) => {
                    self.handle_register_declaration(flags, name, start);
                }
                ASMStatement::InstructionDeclaration(start, name, params, body) => {
                    self.handle_instruction_def(start, body, name, params);
                }
                ASMStatement::InlinePil(_start, statements) => self.pil.extend(statements.clone()),
                ASMStatement::Assignment(start, write_regs, assign_reg, value) => {
                    self.handle_assignment(*start, write_regs, assign_reg, value.as_ref())
                }
                ASMStatement::Instruction(_start, instr_name, args) => {
                    self.handle_instruction(instr_name, args)
                }
                ASMStatement::Label(_start, name) => self.code_lines.push(CodeLine {
                    label: Some(name.clone()),
                    ..Default::default()
                }),
            }
        }
        self.create_constraints_for_assignment_reg();

        self.pil.extend(
            self.registers
                .iter()
                .filter_map(|(name, reg)| reg.update_expression().map(|update| (name, update)))
                .map(|(name, update)| {
                    Statement::PolynomialIdentity(0, build_sub(next_reference(name), update))
                }),
        );

        self.create_fixed_columns_for_program();

        self.pil.push(Statement::PlookupIdentity(
            0,
            SelectedExpressions {
                selector: None,
                expressions: self
                    .line_lookup
                    .iter()
                    .map(|x| direct_reference(&x.0))
                    .collect(),
            },
            SelectedExpressions {
                selector: None,
                expressions: self
                    .line_lookup
                    .iter()
                    .map(|x| direct_reference(&x.1))
                    .collect(),
            },
        ));

        PILFile(std::mem::take(&mut self.pil))
    }

    fn handle_register_declaration(
        &mut self,
        flags: &Option<RegisterFlag>,
        name: &String,
        start: &usize,
    ) {
        let mut conditioned_updates = vec![];
        let mut default_update = None;
        match flags {
            Some(RegisterFlag::IsPC) => {
                assert_eq!(self.pc_name, None);
                self.pc_name = Some(name.clone());
                self.line_lookup.push((name.clone(), "line".to_string()));
                default_update = Some(build_add(direct_reference(name), build_number(1)));
            }
            Some(RegisterFlag::IsDefaultAssignment) => {
                assert_eq!(self.default_assignment, None);
                self.default_assignment = Some(name.clone());
            }
            None => {
                let write_flag = format!("reg_write_{name}");
                self.create_witness_fixed_pair(*start, &write_flag);
                conditioned_updates = vec![(
                    direct_reference(&write_flag),
                    direct_reference(self.default_assignment_reg()),
                )];
                default_update = Some(direct_reference(name));
            }
        };
        self.registers.insert(
            name.clone(),
            Register {
                conditioned_updates,
                default_update,
            },
        );
        self.pil.push(witness_column(*start, name));
    }

    fn handle_instruction_def(
        &mut self,
        start: &usize,
        body: &Vec<Expression>,
        name: &String,
        params: &Vec<InstructionParam>,
    ) {
        let col_name = format!("instr_{name}");
        self.create_witness_fixed_pair(*start, &col_name);
        // it's part of the lookup!
        //self.pil.push(constrain_zero_one(&col_name));

        let mut substitutions = HashMap::new();
        for p in params {
            if p.assignment_reg.0.is_none() && p.assignment_reg.1.is_none() {
                // literal argument
                let param_col_name = format!("instr_{name}_param_{}", p.name);
                self.create_witness_fixed_pair(*start, &param_col_name);
                substitutions.insert(p.name.clone(), param_col_name);
            }
        }

        for expr in body {
            let expr = substitute(expr, &substitutions);
            match extract_update(expr) {
                (Some(var), expr) => {
                    self.registers
                        .get_mut(&var)
                        .unwrap()
                        .conditioned_updates
                        .push((direct_reference(&col_name), expr));
                }
                (None, expr) => self.pil.push(Statement::PolynomialIdentity(
                    0,
                    build_mul(direct_reference(&col_name), expr.clone()),
                )),
            }
        }
        let instr = Instruction {
            params: params.clone(),
        };
        self.instructions.insert(name.clone(), instr);
    }

    fn handle_assignment(
        &mut self,
        _start: usize,
        write_regs: &Vec<String>,
        _assign_reg: &Option<String>,
        value: &Expression,
    ) {
        assert!(write_regs.len() <= 1);
        let value = self.process_assignment_value(value);
        // TODO handle assign register
        self.code_lines.push(CodeLine {
            write_reg: write_regs.first().cloned(),
            value,
            ..Default::default()
        })
    }

    fn handle_instruction(&mut self, instr_name: &String, args: &Vec<Expression>) {
        let instr = &self.instructions[instr_name];
        assert_eq!(instr.params.len(), args.len());
        let mut value = vec![];
        let instruction_literal_args = instr
            .params
            .iter()
            .zip(args)
            .map(|(p, a)| {
                if p.assignment_reg.0.is_some() || p.assignment_reg.1.is_some() {
                    // TODO this ignores which param it is, but it's ok
                    // TODO if we have  more than one assignment op, we cannot just use
                    // value here anymore. But I guess the same is for assignment.
                    // TODO check that we do not use the same assignment var twice
                    value = self.process_assignment_value(a);
                    None
                } else if p.param_type == Some("label".to_string()) {
                    if let Expression::PolynomialReference(r) = a {
                        Some(r.name.clone())
                    } else {
                        panic!();
                    }
                } else {
                    todo!("Param type not supported.");
                }
            })
            .collect();
        self.code_lines.push(CodeLine {
            instruction: Some(instr_name.clone()),
            value,
            instruction_literal_args,
            ..Default::default()
        });
    }

    fn process_assignment_value(
        &self,
        value: &Expression,
    ) -> Vec<(ConstantNumberType, AffineExpressionComponent)> {
        match value {
            Expression::Constant(_) => panic!(),
            Expression::PublicReference(_) => panic!(),
            Expression::FunctionCall(_, _) => panic!(),
            Expression::PolynomialReference(reference) => {
                assert!(reference.namespace.is_none());
                assert!(reference.index.is_none());
                assert!(!reference.next);
                // TODO check it actually is a register
                vec![(
                    1,
                    AffineExpressionComponent::Register(reference.name.clone()),
                )]
            }
            Expression::Number(value) => vec![(*value, AffineExpressionComponent::Constant)],
            Expression::String(_) => panic!(),
            Expression::Tuple(_) => panic!(),
            Expression::FreeInput(expr) => {
                vec![(1, AffineExpressionComponent::FreeInput(*expr.clone()))]
            }
            Expression::BinaryOperation(left, op, right) => match op {
                BinaryOperator::Add => self.add_assignment_value(
                    self.process_assignment_value(left),
                    self.process_assignment_value(right),
                ),
                BinaryOperator::Sub => self.add_assignment_value(
                    self.process_assignment_value(left),
                    self.negate_assignment_value(self.process_assignment_value(right)),
                ),
                BinaryOperator::Mul => todo!(),
                BinaryOperator::Div => panic!(),
                BinaryOperator::Mod => panic!(),
                BinaryOperator::Pow => panic!(),
                BinaryOperator::BinaryAnd => panic!(),
                BinaryOperator::BinaryOr => panic!(),
                BinaryOperator::ShiftLeft => panic!(),
                BinaryOperator::ShiftRight => panic!(),
            },
            Expression::UnaryOperation(op, expr) => {
                assert!(*op == UnaryOperator::Minus);
                self.negate_assignment_value(self.process_assignment_value(expr))
            }
        }
    }

    fn add_assignment_value(
        &self,
        mut left: Vec<(ConstantNumberType, AffineExpressionComponent)>,
        right: Vec<(ConstantNumberType, AffineExpressionComponent)>,
    ) -> Vec<(ConstantNumberType, AffineExpressionComponent)> {
        // TODO combine (or at leats check for) same components.
        left.extend(right);
        left
    }

    fn negate_assignment_value(
        &self,
        expr: Vec<(ConstantNumberType, AffineExpressionComponent)>,
    ) -> Vec<(ConstantNumberType, AffineExpressionComponent)> {
        expr.into_iter().map(|(v, c)| (-v, c)).collect()
    }

    fn create_constraints_for_assignment_reg(&mut self) {
        let assign_const = format!("{}_const", self.default_assignment_reg());
        self.create_witness_fixed_pair(0, &assign_const);
        let read_free = format!("{}_read_free", self.default_assignment_reg());
        self.create_witness_fixed_pair(0, &read_free);
        let free_value = format!("{}_free_value", self.default_assignment_reg());
        self.pil.push(witness_column(0, &free_value));
        let registers = self
            .registers
            .keys()
            .filter(|name| **name != self.default_assignment_reg())
            .cloned()
            .collect::<Vec<_>>();
        let assign_constraint = registers
            .iter()
            .map(|name| {
                let read_coefficient = format!("read_{}_{name}", self.default_assignment_reg());
                self.create_witness_fixed_pair(0, &read_coefficient);
                build_mul(direct_reference(&read_coefficient), direct_reference(name))
            })
            .chain([
                direct_reference(&assign_const),
                build_mul(direct_reference(&read_free), direct_reference(&free_value)),
            ])
            .reduce(build_add);
        self.pil.push(Statement::PolynomialIdentity(
            0,
            build_sub(
                direct_reference(self.default_assignment_reg()),
                assign_constraint.unwrap(),
            ),
        ));
    }

    fn create_fixed_columns_for_program(&mut self) {
        self.pil.push(Statement::PolynomialConstantDefinition(
            0,
            "line".to_string(),
            FunctionDefinition::Mapping(vec!["i".to_string()], direct_reference("i")),
        ));
        // TODO check that all of them are matched against execution trace witnesses.
        let mut program_constants = self
            .program_constant_names
            .iter()
            .map(|n| (n, vec![0; self.code_lines.len()]))
            .collect::<BTreeMap<_, _>>();
        let label_positions = self.compute_label_positions();
        for (i, line) in self.code_lines.iter().enumerate() {
            if let Some(reg) = &line.write_reg {
                program_constants
                    .get_mut(&format!("p_reg_write_{reg}"))
                    .unwrap()[i] = 1.into();
            }
            for (coeff, item) in &line.value {
                match item {
                    AffineExpressionComponent::Register(reg) => {
                        program_constants
                            .get_mut(&format!("p_read_{}_{reg}", self.default_assignment_reg()))
                            .unwrap()[i] = *coeff;
                    }
                    AffineExpressionComponent::Constant => {
                        program_constants
                            .get_mut(&format!("p_{}_const", self.default_assignment_reg()))
                            .unwrap()[i] = *coeff
                    }
                    AffineExpressionComponent::FreeInput(_) => {
                        // The program just stores that we read a free input, the actual value
                        // is part of the execution trace that generates the witness.
                        program_constants
                            .get_mut(&format!("p_{}_read_free", self.default_assignment_reg()))
                            .unwrap()[i] = 1.into();
                    }
                }
            }
            if let Some(instr) = &line.instruction {
                program_constants
                    .get_mut(&format!("p_instr_{instr}"))
                    .unwrap()[i] = 1.into();
                for (arg, param) in line
                    .instruction_literal_args
                    .iter()
                    .zip(&self.instructions[instr].params)
                {
                    if let Some(arg) = arg {
                        // TODO has to be label for now
                        program_constants
                            .get_mut(&format!("p_instr_{instr}_param_{}", param.name))
                            .unwrap()[i] = (label_positions[arg] as i64).into();
                    }
                }
            } else {
                assert!(line.instruction_literal_args.is_empty());
            }
        }
        for (name, values) in program_constants {
            self.pil.push(Statement::PolynomialConstantDefinition(
                0,
                name.clone(),
                FunctionDefinition::Array(values.into_iter().map(build_number).collect()),
            ));
        }
    }

    fn compute_label_positions(&self) -> HashMap<String, usize> {
        self.code_lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| line.label.as_ref().map(|l| (l.clone(), i)))
            .collect()
    }

    /// Creates a pair of witness and fixed column and matches them in the lookup.
    fn create_witness_fixed_pair(&mut self, start: usize, name: &str) {
        let fixed_name = format!("p_{name}");
        self.pil.push(witness_column(start, name));
        self.line_lookup
            .push((name.to_string(), fixed_name.clone()));
        self.program_constant_names.push(fixed_name);
    }

    fn default_assignment_reg(&self) -> &str {
        self.default_assignment.as_ref().unwrap()
    }
}

struct Register {
    /// Constraints to update this register, first item being the
    /// condition, second item the value.
    /// TODO check that condition is bool
    conditioned_updates: Vec<(Expression, Expression)>,
    default_update: Option<Expression>,
}

impl Register {
    /// Returns the expression assigned to this register in the next row.
    pub fn update_expression(&self) -> Option<Expression> {
        // TODO conditions need to be all boolean
        let updates = self
            .conditioned_updates
            .iter()
            .map(|(cond, value)| build_mul(cond.clone(), value.clone()))
            .reduce(build_add);

        match (self.conditioned_updates.len(), &self.default_update) {
            (0, update) => update.clone(),
            (_, None) => Some(updates.unwrap()),
            (_, Some(def)) => {
                let default_condition = build_sub(
                    build_number(1),
                    self.conditioned_updates
                        .iter()
                        .map(|(cond, _value)| cond.clone())
                        .reduce(build_add)
                        .unwrap(),
                );
                Some(build_add(
                    updates.unwrap(),
                    build_mul(default_condition, def.clone()),
                ))
            }
        }
    }
}

struct Instruction {
    params: Vec<InstructionParam>,
}

#[derive(Default)]
struct CodeLine {
    write_reg: Option<String>,
    value: Vec<(ConstantNumberType, AffineExpressionComponent)>,
    label: Option<String>,
    instruction: Option<String>,
    // TODO we only support labels for now.
    instruction_literal_args: Vec<Option<String>>,
}

enum AffineExpressionComponent {
    Register(String),
    Constant,
    FreeInput(Expression),
}

fn witness_column(start: usize, name: &str) -> Statement {
    Statement::PolynomialCommitDeclaration(
        start,
        vec![PolynomialName {
            name: name.to_string(),
            array_size: None,
        }],
        None,
    )
}

fn direct_reference(name: &str) -> Expression {
    Expression::PolynomialReference(PolynomialReference {
        namespace: None,
        name: name.to_owned(),
        index: None,
        next: false,
    })
}

fn next_reference(name: &str) -> Expression {
    Expression::PolynomialReference(PolynomialReference {
        namespace: None,
        name: name.to_owned(),
        index: None,
        next: true,
    })
}

fn build_mul(left: Expression, right: Expression) -> Expression {
    build_binary_expr(left, BinaryOperator::Mul, right)
}

fn build_sub(left: Expression, right: Expression) -> Expression {
    build_binary_expr(left, BinaryOperator::Sub, right)
}

fn build_add(left: Expression, right: Expression) -> Expression {
    build_binary_expr(left, BinaryOperator::Add, right)
}

fn build_binary_expr(left: Expression, op: BinaryOperator, right: Expression) -> Expression {
    Expression::BinaryOperation(Box::new(left), op, Box::new(right))
}

fn build_unary_expr(op: UnaryOperator, exp: Expression) -> Expression {
    Expression::UnaryOperation(op, Box::new(exp))
}

fn build_number(value: i128) -> Expression {
    Expression::Number(value)
}

fn extract_update(expr: Expression) -> (Option<String>, Expression) {
    // TODO check that there are no other "next" references in the expression
    if let Expression::BinaryOperation(left, BinaryOperator::Sub, right) = expr {
        if let Expression::PolynomialReference(PolynomialReference {
            namespace,
            name,
            index,
            next: true,
        }) = *left
        {
            assert_eq!(namespace, None);
            assert_eq!(index, None);
            (Some(name), *right)
        } else {
            (None, build_binary_expr(*left, BinaryOperator::Sub, *right))
        }
    } else {
        (None, expr)
    }
}

fn substitute(input: &Expression, substitution: &HashMap<String, String>) -> Expression {
    match input {
        // TODO namespace
        Expression::PolynomialReference(r) => {
            Expression::PolynomialReference(PolynomialReference {
                name: substitute_string(&r.name, substitution),
                ..r.clone()
            })
        }
        Expression::BinaryOperation(left, op, right) => build_binary_expr(
            substitute(left, substitution),
            *op,
            substitute(right, substitution),
        ),
        Expression::UnaryOperation(op, exp) => build_unary_expr(*op, substitute(exp, substitution)),
        Expression::FunctionCall(name, args) => Expression::FunctionCall(
            name.clone(),
            args.iter().map(|e| substitute(e, substitution)).collect(),
        ),
        Expression::Tuple(items) => {
            Expression::Tuple(items.iter().map(|e| substitute(e, substitution)).collect())
        }
        Expression::Constant(_)
        | Expression::PublicReference(_)
        | Expression::Number(_)
        | Expression::String(_)
        | Expression::FreeInput(_) => input.clone(),
    }
}

fn substitute_string(input: &String, substitution: &HashMap<String, String>) -> String {
    substitution.get(input).unwrap_or(input).clone()
}

#[cfg(test)]
mod test {
    use std::fs;

    use super::compile;

    #[test]
    pub fn compile_simple_sum() {
        let expectation = r#"
pol commit X;
pol commit reg_write_A;
pol commit A;
pol commit reg_write_CNT;
pol commit CNT;
pol commit pc;
pol commit XInv;
pol XIsZero = (1 - (X * XInv));
(XIsZero * X) = 0;
pol commit instr_jmpz;
pol commit instr_jmpz_param_l;
pol commit instr_jmp;
pol commit instr_jmp_param_l;
pol commit instr_dec_CNT;
pol commit instr_assert_zero;
(instr_assert_zero * (XIsZero - 1)) = 0;
pol commit X_const;
pol commit X_read_free;
pol commit X_free_value;
pol commit read_X_A;
pol commit read_X_CNT;
pol commit read_X_pc;
X = (((((read_X_A * A) + (read_X_CNT * CNT)) + (read_X_pc * pc)) + X_const) + (X_read_free * X_free_value));
A' = ((reg_write_A * X) + ((1 - reg_write_A) * A));
CNT' = (((reg_write_CNT * X) + (instr_dec_CNT * (CNT - 1))) + ((1 - (reg_write_CNT + instr_dec_CNT)) * CNT));
pc' = (((instr_jmpz * ((XIsZero * instr_jmpz_param_l) + ((1 - XIsZero) * (pc + 1)))) + (instr_jmp * instr_jmp_param_l)) + ((1 - (instr_jmpz + instr_jmp)) * (pc + 1)));
pol constant line(i) { i };
pol constant p_X_const = [0, 0, 0, 0, 0, 0, 0, 0, 0];
pol constant p_X_read_free = [1, 0, 0, 1, 0, 0, 0, 1, 0];
pol constant p_instr_assert_zero = [0, 0, 0, 0, 0, 0, 0, 0, 1];
pol constant p_instr_dec_CNT = [0, 0, 0, 0, 1, 0, 0, 0, 0];
pol constant p_instr_jmp = [0, 0, 0, 0, 0, 1, 0, 0, 0];
pol constant p_instr_jmp_param_l = [0, 0, 0, 0, 0, 1, 0, 0, 0];
pol constant p_instr_jmpz = [0, 0, 1, 0, 0, 0, 0, 0, 0];
pol constant p_instr_jmpz_param_l = [0, 0, 6, 0, 0, 0, 0, 0, 0];
pol constant p_read_X_A = [0, 0, 0, 1, 0, 0, 0, 1, 1];
pol constant p_read_X_CNT = [0, 0, 1, 0, 0, 0, 0, 0, 0];
pol constant p_read_X_pc = [0, 0, 0, 0, 0, 0, 0, 0, 0];
pol constant p_reg_write_A = [0, 0, 0, 1, 0, 0, 0, 1, 0];
pol constant p_reg_write_CNT = [1, 0, 0, 0, 0, 0, 0, 0, 0];
{ reg_write_A, reg_write_CNT, pc, instr_jmpz, instr_jmpz_param_l, instr_jmp, instr_jmp_param_l, instr_dec_CNT, instr_assert_zero, X_const, X_read_free, read_X_A, read_X_CNT, read_X_pc } in { p_reg_write_A, p_reg_write_CNT, line, p_instr_jmpz, p_instr_jmpz_param_l, p_instr_jmp, p_instr_jmp_param_l, p_instr_dec_CNT, p_instr_assert_zero, p_X_const, p_X_read_free, p_read_X_A, p_read_X_CNT, p_read_X_pc };
"#;
        let file_name = "tests/simple_sum.asm";
        let contents = fs::read_to_string(file_name).unwrap();
        let pil = compile(Some(file_name), &contents).unwrap();
        assert_eq!(format!("{pil}").trim(), expectation.trim());
    }
}