macro_rules! owned_uuid_reference {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(::uuid::Uuid);

        impl $name {
            pub fn try_from_uuid(
                value: ::uuid::Uuid,
            ) -> Result<Self, $crate::ContractValidationError> {
                if value.is_nil() {
                    return Err($crate::ContractValidationError::NilIdentifier);
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> ::uuid::Uuid {
                self.0
            }
        }

        impl TryFrom<::uuid::Uuid> for $name {
            type Error = $crate::ContractValidationError;

            fn try_from(value: ::uuid::Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for ::uuid::Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                <::uuid::Uuid as ::serde::Serialize>::serialize(&self.0, serializer)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                let value = <::uuid::Uuid as ::serde::Deserialize>::deserialize(deserializer)?;
                Self::try_from_uuid(value).map_err(::serde::de::Error::custom)
            }
        }
    };
}

macro_rules! impl_integration_event {
    ($event:ty, $owner:path, { $($pattern:pat => $event_type:literal),+ $(,)? }) => {
        impl $crate::IntegrationEvent for $event {
            fn owner(&self) -> $crate::BoundedContext {
                $owner
            }

            fn event_type(&self) -> &'static str {
                match self {
                    $($pattern => $event_type),+
                }
            }

            fn schema_version(&self) -> u16 {
                1
            }
        }
    };
}
