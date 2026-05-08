macro_rules! byte_array_newtype {
    ($name:ident, $size:expr, copy) => {
        #[derive(alloy_rlp::RlpEncodable, alloy_rlp::RlpDecodable, Copy, Clone)]
        pub struct $name([u8; $size]);

        byte_array_newtype_impls!($name, $size);
    };

    ($name:ident, $size:expr, no_copy) => {
        #[derive(Clone)]
        pub struct $name([u8; $size]);

        impl BitOrAssign<&Bloom> for Bloom {
            fn bitor_assign(&mut self, rhs: &Bloom) {
                for (a, b) in self.0.iter_mut().zip(rhs.0.iter()) {
                    *a |= *b;
                }
            }
        }

        byte_array_newtype_impls!($name, $size);
    };
}

macro_rules! byte_array_newtype_impls {
    ($name:ident, $size:expr) => {
        impl Default for $name {
            fn default() -> Self {
                Self([0; $size])
            }
        }

        impl Eq for $name {}

        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }
        impl Hash for $name {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.0.hash(state);
            }
        }

        impl From<[u8; $size]> for $name {
            fn from(bytes: [u8; $size]) -> Self {
                Self(bytes)
            }
        }

        impl AsRef<[u8]> for $name {
            fn as_ref(&self) -> &[u8] {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "0x")?;
                for byte in &self.0 {
                    write!(f, "{:02x}", byte)?;
                }
                Ok(())
            }
        }

        impl fmt::LowerHex for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                if f.alternate() {
                    write!(f, "0x")?;
                }
                for byte in &self.0 {
                    write!(f, "{:02x}", byte)?;
                }
                Ok(())
            }
        }

        impl fmt::UpperHex for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                if f.alternate() {
                    write!(f, "0x")?;
                }
                for byte in &self.0 {
                    write!(f, "{:02X}", byte)?;
                }
                Ok(())
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self)?;
                Ok(())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serde::Serialize::serialize(self.0.as_slice(), serializer)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let bytes = <Vec<u8> as serde::Deserialize>::deserialize(deserializer)?;
                Self::try_from(bytes.as_slice()).map_err(serde::de::Error::custom)
            }
        }

        impl TryFrom<&[u8]> for $name {
            type Error = error::ParseError;
            fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
                let addr: [u8; $size] = value
                    .try_into()
                    .map_err(|_| error::ParseError::WrongLength($size, value.len()))?;
                Ok($name(addr))
            }
        }

        impl FromStr for $name {
            type Err = error::ParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $name::try_from(error::decode_hex(s)?.as_slice())
            }
        }

        impl $name {
            pub fn new(bytes: [u8; $size]) -> Self {
                Self::from(bytes)
            }

            pub fn zero() -> Self {
                Self::default()
            }

            pub fn as_bytes(&self) -> &[u8; $size] {
                &self.0
            }
        }
    };
}

macro_rules! evm_opcodes {
    ($($name:ident = $value:expr),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum Opcode {
            $(
                $name = $value,
            )*
        }

        impl Opcode {
            pub fn byte(&self) -> u8 {
                *self as u8
            }

            pub fn from_byte(byte: u8) -> Result<Self, OpcodeError> {
                match byte {
                    $(
                        $value => Ok(Self::$name),
                    )*
                    _ => Err(OpcodeError::UnknownOpcode(byte)),
                }
            }
        }

        impl std::fmt::Display for Opcode {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let name = match self {
                    $(
                        Self::$name => stringify!($name),
                    )*
                };
                f.write_str(name)
            }
        }
    };
}

macro_rules! impl_type_name {
    ($($name:ident => $str:expr),*$(,)?) => {
        $(
            impl TypeName for $name {
                fn type_name() -> &'static str {
                    $str
                }
            }
        )*
    };
}
