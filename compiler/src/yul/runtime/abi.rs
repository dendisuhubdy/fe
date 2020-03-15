use crate::errors::CompileError;
use crate::yul::namespace::scopes::ContractDef;
use crate::yul::namespace::types::{Array, Base, FixedSize, Type};

use std::collections::HashMap;
use tiny_keccak::{Hasher, Keccak};
use yultsur::*;

/// Builds a switch statement from the contract ABI.
/// The switch's expression is the 4 left-most bytes in the calldata and each case is
/// defined as the keccak value of each function's signature (without return data).
pub fn switch(
    interface: &Vec<String>,
    defs: &HashMap<String, ContractDef>,
) -> Result<yul::Statement, CompileError> {
    let cases = interface
        .into_iter()
        .map(|name| case(name.to_owned(), defs))
        .collect::<Result<Vec<yul::Case>, CompileError>>()?;

    Ok(switch! {
        switch (callval(0, 4))
        [cases...]
    })
}

pub fn case(name: String, defs: &HashMap<String, ContractDef>) -> Result<yul::Case, CompileError> {
    if let Some(def) = defs.get(&name) {
        return match def {
            ContractDef::Function { params, returns } => {
                Ok(function_call_case(name, params, returns.to_owned()))
            }
            _ => Err(CompileError::static_str(
                "Cannot create case from definition",
            )),
        };
    }

    Err(CompileError::static_str("No definition for name"))
}

/// Builds a switch case from the function. It matches the selector and calls
/// the function as described in the ABI. The value (if any) returned by the
/// function is stored in memory and returned by the contract.
///
/// Currently, this assumes each input and the single output is 256 bits.
/// TODO: Handle types of different sizes: https://solidity.readthedocs.io/en/v0.6.2/abi-spec.html#types
pub fn function_call_case(
    name: String,
    params: &Vec<FixedSize>,
    returns: Option<FixedSize>,
) -> yul::Case {
    let selector = selector_literal(name.clone(), &params);
    let name = identifier! {(name)};
    let params = parameter_expressions(&params);

    if let Some(returns) = returns {
        let return_size = literal_expression! {(returns.size())};

        match returns {
            FixedSize::Array(_) => case! {
                case [selector] {
                    (let return_ptr := [name]([params...]))
                    (return(return_ptr, [return_size]))
                }
            },
            FixedSize::Base(_) => case! {
                case [selector] {
                    (let return_val := [name]([params...]))
                    (mstore(0, return_val))
                    (return(0, [return_size]))
                }
            },
        }
    } else {
        case! {
            case [selector] {
                ([name]([params...]))
            }
        }
    }
}

/// Computes the keccak-256 value of the input portion of the function signature and returns the
/// first 4 bytes.
///
/// Example: "foo(uint256):(uint256)" => keccak256("foo(uint256)")
pub fn selector_literal(name: String, params: &Vec<FixedSize>) -> yul::Literal {
    let signature = format!(
        "{}({})",
        name,
        params
            .iter()
            .map(|param| abi_type(param.to_owned()))
            .collect::<Vec<String>>()
            .join(",")
    );

    let mut keccak = Keccak::v256();
    let mut selector = [0u8; 4];

    keccak.update(signature.as_bytes());
    keccak.finalize(&mut selector);

    literal! {(format!("0x{}", hex::encode(selector)))}
}

pub fn abi_type_base(typ: Base) -> String {
    match typ {
        Base::U256 => "uint256".to_string(),
        Base::Address => "address".to_string(),
        Base::Byte => "byte".to_string(),
    }
}

pub fn abi_type(typ: FixedSize) -> String {
    match typ {
        FixedSize::Base(base) => abi_type_base(base),
        FixedSize::Array(Array { dimension, inner }) => {
            if inner == Base::Byte {
                return format!("bytes{}", dimension);
            }

            format!("{}[{}]", abi_type_base(inner), dimension)
        }
    }
}

/// Creates a Vec of Yul expressions that loads each parameter from calldata.
pub fn parameter_expressions(params: &Vec<FixedSize>) -> Vec<yul::Expression> {
    let mut ptr = 4;
    let mut expressions = vec![];

    for param in params.iter() {
        let start = literal_expression! {(ptr)};
        let size = literal_expression! {(param.size())};
        ptr += param.size();

        expressions.push(match param {
            FixedSize::Base(base) => expression! { callval([start], [size]) },
            FixedSize::Array(array) => expression! { calltomem([start], [size]) },
        });
    }

    expressions
}

/*
#[cfg(test)]
mod tests {
    use crate::yul::runtime::abi::selector_literal;
    use crate::yul::types;

    /*
    #[test]
    fn test_selector_literal_basic() {
        let json_abi = r#"[{"name": "foo", "type": "function", "inputs": [], "outputs": []}]"#;
        let abi = ethabi::Contract::load(StringReader::new(json_abi)).expect("Unable to load abi.");
        let ref foo = abi.functions["foo"][0];

        assert_eq!(
            selector_literal(foo.signature()).to_string(),
            String::from("0xc2985578"),
            "Incorrect selector"
        )
    }
    */

    #[test]
    fn test_selector_literal() {
        assert_eq!(
            selector_literal(
                "bar".to_string(),
                &vec![FixedSize::Base(Base::U256)]
            )
            .to_string(),
            String::from("0x0423a132"),
        )
    }

    /*
    #[test]
    fn test_case() {
        let json_abi = r#"[{"name": "foo", "type": "function", "inputs": [{ "name": "bar", "type": "uint256" }], "outputs": [{ "name": "baz", "type": "uint256" }]}]"#;
        let abi = ethabi::Contract::load(StringReader::new(json_abi)).expect("Unable to load abi.");
        let ref foo = abi.functions["foo"][0];

        assert_eq!(
            case(foo).expect("Unable to build case.").to_string(),
            String::from("case 0x2fbebd38 { mstore(0, foo(calldataload(4))) return(0, 32) }"),
        );
    }

    #[test]
    fn test_switch_basic() {
        let json_abi = r#"[{"name": "foo", "type": "function", "inputs": [], "outputs": []}]"#;
        let abi = ethabi::Contract::load(StringReader::new(json_abi)).expect("Unable to load abi.");
        let functions = abi.functions().collect();

        assert_eq!(
            switch(functions)
                .expect("Unable to build selector")
                .to_string(),
            String::from("switch callval(0, 4) case 0xc2985578 { foo() } "),
        )
    }
    */
}
*/
