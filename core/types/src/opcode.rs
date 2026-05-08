use thiserror::Error;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum OpcodeError {
    #[error("unknown opcode: 0x{0:02x}")]
    UnknownOpcode(u8),
}

evm_opcodes! {
    STOP = 0x00,
    ADD = 0x01,
    MUL = 0x02,
    SUB = 0x03,
    SLOAD = 0x54,
    SSTORE = 0x55,
    PUSH1 = 0x60,
    CALL = 0xf1,
    RETURN = 0xf3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_byte_returns_discriminant() {
        assert_eq!(Opcode::ADD.byte(), 0x01);
    }

    #[test]
    fn from_byte_converts_known_opcode() {
        assert_eq!(Opcode::from_byte(0x00), Ok(Opcode::STOP));
        assert_eq!(Opcode::from_byte(0x60), Ok(Opcode::PUSH1));
    }

    #[test]
    fn from_byte_rejects_unknown_opcode() {
        assert_eq!(
            Opcode::from_byte(0xff),
            Err(OpcodeError::UnknownOpcode(0xff))
        );
    }

    #[test]
    fn display_returns_variant_name() {
        assert_eq!(format!("{}", Opcode::CALL), "CALL");
    }

    #[test]
    fn defined_opcodes_round_trip() {
        let opcodes = [
            Opcode::STOP,
            Opcode::ADD,
            Opcode::MUL,
            Opcode::SUB,
            Opcode::SLOAD,
            Opcode::SSTORE,
            Opcode::PUSH1,
            Opcode::CALL,
            Opcode::RETURN,
        ];

        for opcode in opcodes {
            assert_eq!(Opcode::from_byte(opcode.byte()), Ok(opcode));
        }
    }
}
