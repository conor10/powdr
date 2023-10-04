use std::ops::ControlFlow;

use super::{
    ArrayExpression, ArrayLiteral, Expression, FunctionCall, FunctionDefinition, LambdaExpression,
    MatchArm, MatchPattern, PilStatement, SelectedExpressions, ShiftedPolynomialReference,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisitOrder {
    Pre,
    Post,
}

/// A trait to be implemented by an AST node.
/// The idea is that it calls a callback function on each of the sub-nodes
/// that are expressions.
pub trait ExpressionVisitable<T, Ref> {
    /// Traverses the AST and calls `f` on each Expression in pre-order,
    /// potentially break early and return a value.
    fn pre_visit_expressions_return_mut<F, B>(&mut self, f: &mut F) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.visit_expressions_mut(f, VisitOrder::Pre)
    }

    /// Traverses the AST and calls `f` on each Expression in pre-order.
    fn pre_visit_expressions_mut<F>(&mut self, f: &mut F)
    where
        F: FnMut(&mut Expression<T, Ref>),
    {
        self.pre_visit_expressions_return_mut(&mut move |e| {
            f(e);
            ControlFlow::Continue::<()>(())
        });
    }

    /// Traverses the AST and calls `f` on each Expression in pre-order,
    /// potentially break early and return a value.
    fn pre_visit_expressions_return<F, B>(&self, f: &mut F) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.visit_expressions(f, VisitOrder::Pre)
    }

    /// Traverses the AST and calls `f` on each Expression in pre-order.
    fn pre_visit_expressions<F>(&self, f: &mut F)
    where
        F: FnMut(&Expression<T, Ref>),
    {
        self.pre_visit_expressions_return(&mut move |e| {
            f(e);
            ControlFlow::Continue::<()>(())
        });
    }

    /// Traverses the AST and calls `f` on each Expression in post-order,
    /// potentially break early and return a value.
    fn post_visit_expressions_return_mut<F, B>(&mut self, f: &mut F) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.visit_expressions_mut(f, VisitOrder::Post)
    }

    /// Traverses the AST and calls `f` on each Expression in post-order.
    fn post_visit_expressions_mut<F>(&mut self, f: &mut F)
    where
        F: FnMut(&mut Expression<T, Ref>),
    {
        self.post_visit_expressions_return_mut(&mut move |e| {
            f(e);
            ControlFlow::Continue::<()>(())
        });
    }

    /// Traverses the AST and calls `f` on each Expression in post-order,
    /// potentially break early and return a value.
    fn post_visit_expressions_return<F, B>(&self, f: &mut F) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.visit_expressions(f, VisitOrder::Post)
    }

    /// Traverses the AST and calls `f` on each Expression in post-order.
    fn post_visit_expressions<F>(&self, f: &mut F)
    where
        F: FnMut(&Expression<T, Ref>),
    {
        self.post_visit_expressions_return(&mut move |e| {
            f(e);
            ControlFlow::Continue::<()>(())
        });
    }

    fn visit_expressions<F, B>(&self, f: &mut F, order: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T, Ref>) -> ControlFlow<B>;

    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, order: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, Ref>) -> ControlFlow<B>;
}

impl<T, Ref> ExpressionVisitable<T, Ref> for Expression<T, Ref> {
    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, Ref>) -> ControlFlow<B>,
    {
        if o == VisitOrder::Pre {
            f(self)?;
        }
        match self {
            Expression::Reference(_)
            | Expression::Constant(_)
            | Expression::PublicReference(_)
            | Expression::Number(_)
            | Expression::String(_) => {}
            Expression::BinaryOperation(left, _, right) => {
                left.visit_expressions_mut(f, o)?;
                right.visit_expressions_mut(f, o)?;
            }
            Expression::FreeInput(e) | Expression::UnaryOperation(_, e) => {
                e.visit_expressions_mut(f, o)?
            }
            Expression::LambdaExpression(lambda) => lambda.visit_expressions_mut(f, o)?,
            Expression::ArrayLiteral(array_literal) => array_literal.visit_expressions_mut(f, o)?,
            Expression::FunctionCall(function) => function.visit_expressions_mut(f, o)?,
            Expression::Tuple(items) => items
                .iter_mut()
                .try_for_each(|item| item.visit_expressions_mut(f, o))?,
            Expression::MatchExpression(scrutinee, arms) => {
                scrutinee.visit_expressions_mut(f, o)?;
                arms.iter_mut()
                    .try_for_each(|arm| arm.visit_expressions_mut(f, o))?;
            }
        };
        if o == VisitOrder::Post {
            f(self)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_expressions<F, B>(&self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T, Ref>) -> ControlFlow<B>,
    {
        if o == VisitOrder::Pre {
            f(self)?;
        }
        match self {
            Expression::Reference(_)
            | Expression::Constant(_)
            | Expression::PublicReference(_)
            | Expression::Number(_)
            | Expression::String(_) => {}
            Expression::BinaryOperation(left, _, right) => {
                left.visit_expressions(f, o)?;
                right.visit_expressions(f, o)?;
            }
            Expression::FreeInput(e) | Expression::UnaryOperation(_, e) => {
                e.visit_expressions(f, o)?
            }
            Expression::LambdaExpression(lambda) => lambda.visit_expressions(f, o)?,
            Expression::ArrayLiteral(array_literal) => array_literal.visit_expressions(f, o)?,
            Expression::FunctionCall(function) => function.visit_expressions(f, o)?,
            Expression::Tuple(items) => items
                .iter()
                .try_for_each(|item| item.visit_expressions(f, o))?,
            Expression::MatchExpression(scrutinee, arms) => {
                scrutinee.visit_expressions(f, o)?;
                arms.iter()
                    .try_for_each(|arm| arm.visit_expressions(f, o))?;
            }
        };
        if o == VisitOrder::Post {
            f(self)?;
        }
        ControlFlow::Continue(())
    }
}

impl<T> ExpressionVisitable<T, ShiftedPolynomialReference<T>> for PilStatement<T> {
    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, ShiftedPolynomialReference<T>>) -> ControlFlow<B>,
    {
        match self {
            PilStatement::FunctionCall(_, _, arguments) => arguments
                .iter_mut()
                .try_for_each(|e| e.visit_expressions_mut(f, o)),
            PilStatement::PlookupIdentity(_, left, right)
            | PilStatement::PermutationIdentity(_, left, right) => [left, right]
                .into_iter()
                .try_for_each(|e| e.visit_expressions_mut(f, o)),
            PilStatement::ConnectIdentity(_start, left, right) => left
                .iter_mut()
                .chain(right.iter_mut())
                .try_for_each(|e| e.visit_expressions_mut(f, o)),

            PilStatement::Namespace(_, _, e)
            | PilStatement::PolynomialDefinition(_, _, e)
            | PilStatement::PolynomialIdentity(_, e)
            | PilStatement::PublicDeclaration(_, _, _, e)
            | PilStatement::ConstantDefinition(_, _, e)
            | PilStatement::LetStatement(_, _, Some(e)) => e.visit_expressions_mut(f, o),

            PilStatement::PolynomialConstantDefinition(_, _, fundef)
            | PilStatement::PolynomialCommitDeclaration(_, _, Some(fundef)) => {
                fundef.visit_expressions_mut(f, o)
            }
            PilStatement::PolynomialCommitDeclaration(_, _, None)
            | PilStatement::Include(_, _)
            | PilStatement::PolynomialConstantDeclaration(_, _)
            | PilStatement::MacroDefinition(_, _, _, _, _)
            | PilStatement::LetStatement(_, _, None) => ControlFlow::Continue(()),
        }
    }

    fn visit_expressions<F, B>(&self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T>) -> ControlFlow<B>,
    {
        match self {
            PilStatement::FunctionCall(_, _, arguments) => {
                arguments.iter().try_for_each(|e| e.visit_expressions(f, o))
            }
            PilStatement::PlookupIdentity(_, left, right)
            | PilStatement::PermutationIdentity(_, left, right) => [left, right]
                .into_iter()
                .try_for_each(|e| e.visit_expressions(f, o)),
            PilStatement::ConnectIdentity(_start, left, right) => left
                .iter()
                .chain(right.iter())
                .try_for_each(|e| e.visit_expressions(f, o)),

            PilStatement::Namespace(_, _, e)
            | PilStatement::PolynomialDefinition(_, _, e)
            | PilStatement::PolynomialIdentity(_, e)
            | PilStatement::PublicDeclaration(_, _, _, e)
            | PilStatement::ConstantDefinition(_, _, e)
            | PilStatement::LetStatement(_, _, Some(e)) => e.visit_expressions(f, o),

            PilStatement::PolynomialConstantDefinition(_, _, fundef)
            | PilStatement::PolynomialCommitDeclaration(_, _, Some(fundef)) => {
                fundef.visit_expressions(f, o)
            }
            PilStatement::PolynomialCommitDeclaration(_, _, None)
            | PilStatement::Include(_, _)
            | PilStatement::PolynomialConstantDeclaration(_, _)
            | PilStatement::MacroDefinition(_, _, _, _, _)
            | PilStatement::LetStatement(_, _, None) => ControlFlow::Continue(()),
        }
    }
}

impl<T, Ref> ExpressionVisitable<T, Ref> for SelectedExpressions<T, Ref> {
    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.selector
            .as_mut()
            .into_iter()
            .chain(self.expressions.iter_mut())
            .try_for_each(move |item| item.visit_expressions_mut(f, o))
    }

    fn visit_expressions<F, B>(&self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.selector
            .as_ref()
            .into_iter()
            .chain(self.expressions.iter())
            .try_for_each(move |item| item.visit_expressions(f, o))
    }
}

impl<T> ExpressionVisitable<T, ShiftedPolynomialReference<T>> for FunctionDefinition<T> {
    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T>) -> ControlFlow<B>,
    {
        match self {
            FunctionDefinition::Query(_, e) | FunctionDefinition::Mapping(_, e) => {
                e.visit_expressions_mut(f, o)
            }
            FunctionDefinition::Array(ae) => ae.visit_expressions_mut(f, o),
            FunctionDefinition::Expression(e) => e.visit_expressions_mut(f, o),
        }
    }

    fn visit_expressions<F, B>(&self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T>) -> ControlFlow<B>,
    {
        match self {
            FunctionDefinition::Query(_, e) | FunctionDefinition::Mapping(_, e) => {
                e.visit_expressions(f, o)
            }
            FunctionDefinition::Array(ae) => ae.visit_expressions(f, o),
            FunctionDefinition::Expression(e) => e.visit_expressions(f, o),
        }
    }
}

impl<T> ExpressionVisitable<T, ShiftedPolynomialReference<T>> for ArrayExpression<T> {
    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T>) -> ControlFlow<B>,
    {
        match self {
            ArrayExpression::Value(expressions) | ArrayExpression::RepeatedValue(expressions) => {
                expressions
                    .iter_mut()
                    .try_for_each(|e| e.visit_expressions_mut(f, o))
            }
            ArrayExpression::Concat(a1, a2) => [a1, a2]
                .iter_mut()
                .try_for_each(|e| e.visit_expressions_mut(f, o)),
        }
    }

    fn visit_expressions<F, B>(&self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T>) -> ControlFlow<B>,
    {
        match self {
            ArrayExpression::Value(expressions) | ArrayExpression::RepeatedValue(expressions) => {
                expressions
                    .iter()
                    .try_for_each(|e| e.visit_expressions(f, o))
            }
            ArrayExpression::Concat(a1, a2) => {
                [a1, a2].iter().try_for_each(|e| e.visit_expressions(f, o))
            }
        }
    }
}

impl<T, Ref> ExpressionVisitable<T, Ref> for LambdaExpression<T, Ref> {
    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.body.visit_expressions_mut(f, o)
    }

    fn visit_expressions<F, B>(&self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.body.visit_expressions(f, o)
    }
}

impl<T, Ref> ExpressionVisitable<T, Ref> for ArrayLiteral<T, Ref> {
    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.items
            .iter_mut()
            .try_for_each(|item| item.visit_expressions_mut(f, o))
    }

    fn visit_expressions<F, B>(&self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.items
            .iter()
            .try_for_each(|item| item.visit_expressions(f, o))
    }
}

impl<T, Ref> ExpressionVisitable<T, Ref> for FunctionCall<T, Ref> {
    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.arguments
            .iter_mut()
            .try_for_each(|item| item.visit_expressions_mut(f, o))
    }

    fn visit_expressions<F, B>(&self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.arguments
            .iter()
            .try_for_each(|item| item.visit_expressions(f, o))
    }
}

impl<T, Ref> ExpressionVisitable<T, Ref> for MatchArm<T, Ref> {
    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.pattern.visit_expressions_mut(f, o)?;
        self.value.visit_expressions_mut(f, o)
    }

    fn visit_expressions<F, B>(&self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T, Ref>) -> ControlFlow<B>,
    {
        self.pattern.visit_expressions(f, o)?;
        self.value.visit_expressions(f, o)
    }
}

impl<T, Ref> ExpressionVisitable<T, Ref> for MatchPattern<T, Ref> {
    fn visit_expressions_mut<F, B>(&mut self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&mut Expression<T, Ref>) -> ControlFlow<B>,
    {
        match self {
            MatchPattern::CatchAll => ControlFlow::Continue(()),
            MatchPattern::Pattern(e) => e.visit_expressions_mut(f, o),
        }
    }

    fn visit_expressions<F, B>(&self, f: &mut F, o: VisitOrder) -> ControlFlow<B>
    where
        F: FnMut(&Expression<T, Ref>) -> ControlFlow<B>,
    {
        match self {
            MatchPattern::CatchAll => ControlFlow::Continue(()),
            MatchPattern::Pattern(e) => e.visit_expressions(f, o),
        }
    }
}