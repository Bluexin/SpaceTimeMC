use spacetimedb::SpacetimeType;
use spacetimedb::sats::meta_type::MetaType;
use spacetimedb::sats::typespace::TypespaceBuilder;
use spacetimedb::sats::{AlgebraicType, impl_deserialize, impl_serialize, impl_st};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Copy, Clone)]
pub struct UUID(Uuid);

impl UUID {
    pub fn new(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_u128(&self) -> u128 {
        self.0.as_u128()
    }
}

impl_serialize!([] UUID, (self, ser) => ser.serialize_u128(self.0.as_u128()));
impl_deserialize!([] UUID, de => u128::deserialize(de).map(Uuid::from_u128).map(UUID));
impl_st!([] UUID, UUID::meta_type());

impl Display for UUID {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl MetaType for UUID {
    fn meta_type() -> AlgebraicType {
        AlgebraicType::U128
    }
}

impl FromStr for UUID {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::from_str(s).map(Self::new).map_err(|e| e.to_string())
    }
}
