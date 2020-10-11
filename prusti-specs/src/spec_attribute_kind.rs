use std::convert::TryFrom;

/// This type identifies one of the procedural macro attributes of Prusti
#[derive(Copy, Clone, Debug)]
pub enum SpecAttributeKind {
    Requires,
    Ensures,
    Pledge,
    Pure,
    Trusted,
}

impl TryFrom<String> for SpecAttributeKind {
    type Error = String;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        match name.as_str() {
            "requires" => Ok(SpecAttributeKind::Requires),
            "ensures" => Ok(SpecAttributeKind::Ensures),
            "pledge" => Ok(SpecAttributeKind::Pledge),
            "pure" => Ok(SpecAttributeKind::Pure),
            "trusted" => Ok(SpecAttributeKind::Trusted),
            _ => Err(name),
        }
    }
}
