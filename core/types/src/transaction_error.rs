use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("Unexpected prefix found: {0}")]
    UnexpectedPrefix(u8),
    #[error("Expected input length is {expected}, but found {actual}")]
    InputTooShort { expected: usize, actual: usize },
    #[error("Invalid structure: {0}")]
    InvalidStructure(String),
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("Insufficient balance: available {available}, required {required}")]
    InsufficientBalance { available: u128, required: u128 },
    #[error("Out of gas: limit {limit}, used {used}")]
    OutOfGas { limit: u64, used: u64 },
    #[error("Invalid nonce: expected {expected}, found {actual}")]
    InvalidNonce { expected: u64, actual: u64 },
    #[error("Overflow error")]
    Overflow,
    #[error("Insufficient max fee: base {base_fee}, max {max_fee}")]
    InsufficientMaxFee { base_fee: u128, max_fee: u128 },
    // `#[from]` generates `impl From<DecodeError> for TransactionError`, which lets
    // the `?` operator automatically convert `DecodeError` into `TransactionError`
    // in any function returning Result<_, TransactionError>.
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_error() {
        let unexpected_prefix = DecodeError::UnexpectedPrefix(100);
        assert_eq!(
            unexpected_prefix.to_string(),
            "Unexpected prefix found: 100"
        );

        let input_too_short = DecodeError::InputTooShort {
            expected: 10,
            actual: 5,
        };
        assert_eq!(
            input_too_short.to_string(),
            "Expected input length is 10, but found 5"
        );

        let invalid_structure = DecodeError::InvalidStructure("Invalid RLP encoding".to_string());
        assert_eq!(
            invalid_structure.to_string(),
            "Invalid structure: Invalid RLP encoding"
        );

        let other_error = DecodeError::Other(Box::new(std::io::Error::other("IO error")));
        assert_eq!(other_error.to_string(), "IO error");
    }

    #[test]
    fn test_execution_error() {
        let insufficient_balance = TransactionError::InsufficientBalance {
            available: 100,
            required: 150,
        };
        assert_eq!(
            insufficient_balance.to_string(),
            "Insufficient balance: available 100, required 150"
        );

        let out_of_gas = TransactionError::OutOfGas {
            limit: 21000,
            used: 22000,
        };
        assert_eq!(
            out_of_gas.to_string(),
            "Out of gas: limit 21000, used 22000"
        );

        let invalid_nonce = TransactionError::InvalidNonce {
            expected: 1,
            actual: 0,
        };
        assert_eq!(
            invalid_nonce.to_string(),
            "Invalid nonce: expected 1, found 0"
        );

        let overflow = TransactionError::Overflow;
        assert_eq!(overflow.to_string(), "Overflow error");

        let insufficient_max_fee = TransactionError::InsufficientMaxFee {
            base_fee: 101,
            max_fee: 100,
        };
        assert_eq!(
            insufficient_max_fee.to_string(),
            "Insufficient max fee: base 101, max 100"
        );

        let execution_error_from_decode: TransactionError =
            TransactionError::Decode(DecodeError::UnexpectedPrefix(100));
        assert_eq!(
            execution_error_from_decode.to_string(),
            "Unexpected prefix found: 100"
        );
    }

    #[test]
    fn test_from_decode_error_via_question_mark() {
        fn helper() -> Result<(), TransactionError> {
            Err(DecodeError::InputTooShort {
                expected: 32,
                actual: 20,
            })?
        }

        let err = helper().unwrap_err();
        assert_eq!(err.to_string(), "Expected input length is 32, but found 20");
    }
}
