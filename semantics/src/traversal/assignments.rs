use crate::errors::SemanticError;
use crate::namespace::scopes::{
    FunctionScope,
    Shared,
};
use crate::traversal::expressions;
use crate::Context;
use fe_parser::ast as fe;
use fe_parser::span::Spanned;
use std::rc::Rc;

/// Gather context information for assignments and check for type errors.
///
/// e.g. `foo[42] = "bar"`, `self.foo[42] = "bar"`, `foo = 42`
pub fn assign(
    scope: Shared<FunctionScope>,
    context: Shared<Context>,
    stmt: &Spanned<fe::FuncStmt>,
) -> Result<(), SemanticError> {
    if let fe::FuncStmt::Assign { targets, value } = &stmt.node {
        if targets.len() > 1 {
            unimplemented!()
        }

        if let Some(target) = targets.first() {
            match &target.node {
                fe::Expr::Name(_) => assign_name(scope, Rc::clone(&context), target, value)?,
                fe::Expr::Subscript { .. } => {
                    assign_subscript(scope, Rc::clone(&context), target, value)?
                }
                _ => return Err(SemanticError::UnassignableExpression),
            }
        }

        return Ok(());
    }

    unreachable!()
}

/// Gather context information for subscript assignments and check for type
/// errors.
///
/// e.g. `foo[42] = "bar"`, `self.foo[42] = "bar"`
fn assign_subscript(
    scope: Shared<FunctionScope>,
    context: Shared<Context>,
    target: &Spanned<fe::Expr>,
    value: &Spanned<fe::Expr>,
) -> Result<(), SemanticError> {
    if let fe::Expr::Subscript {
        value: target,
        slices,
    } = &target.node
    {
        let _target_attributes = expressions::expr(Rc::clone(&scope), Rc::clone(&context), target)?;
        let _value_attributes = expressions::expr(Rc::clone(&scope), Rc::clone(&context), value)?;
        let _index_attributes = expressions::slices_index(scope, context, slices)?;

        // TODO: perform type checking

        return Ok(());
    }

    unreachable!()
}

/// Gather context information for named assignments and check for type errors.
///
/// e.g. `foo = 42`
fn assign_name(
    scope: Shared<FunctionScope>,
    context: Shared<Context>,
    target: &Spanned<fe::Expr>,
    value: &Spanned<fe::Expr>,
) -> Result<(), SemanticError> {
    let _target_attributes = expressions::expr(Rc::clone(&scope), Rc::clone(&context), target)?;
    let _value_attributes = expressions::expr(scope, context, value)?;

    // TODO:: Perform type checking

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::namespace::scopes::{
        ContractScope,
        FunctionScope,
        ModuleScope,
        Shared,
    };
    use crate::namespace::types::{
        Array,
        Base,
        FixedSize,
        Map,
    };
    use crate::traversal::assignments::assign;
    use crate::Context;
    use fe_parser as parser;
    use rstest::rstest;
    use std::rc::Rc;

    fn scope() -> Shared<FunctionScope> {
        let module_scope = ModuleScope::new();
        let contract_scope = ContractScope::new(module_scope);
        contract_scope.borrow_mut().add_map(
            "foobar".to_string(),
            Map {
                key: FixedSize::Base(Base::U256),
                value: FixedSize::Base(Base::U256),
            },
        );
        let function_scope = FunctionScope::new(contract_scope);
        function_scope
            .borrow_mut()
            .add_base("foo".to_string(), Base::U256);
        function_scope.borrow_mut().add_array(
            "bar".to_string(),
            Array {
                inner: Base::U256,
                dimension: 100,
            },
        );
        function_scope
    }

    fn analyze(scope: Shared<FunctionScope>, src: &str) -> Context {
        let context = Context::new_shared();
        let tokens = parser::get_parse_tokens(src).expect("Couldn't parse expression");
        let assignment = &parser::parsers::assign_stmt(&tokens[..])
            .expect("Couldn't build assigment AST")
            .1;

        assign(scope, Rc::clone(&context), assignment).expect("Couldn't map assignment AST");
        Rc::try_unwrap(context)
            .map_err(|_| "")
            .unwrap()
            .into_inner()
    }

    #[rstest(
        assignment,
        expected_num_expr_attrs,
        case("foo = 42", 2),
        case("bar[26] = 42", 3),
        case("self.foobar[26 + 26] = 42", 5)
    )]
    fn assigns(assignment: &str, expected_num_expr_attrs: usize) {
        let context = analyze(scope(), assignment);
        assert_eq!(context.expressions.len(), expected_num_expr_attrs)
    }
}
