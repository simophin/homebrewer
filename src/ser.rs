use derive_more::Deref;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt::Formatter;
use std::marker::PhantomData;

#[derive(Deref)]
pub struct ConfigMap<T>(Vec<T>);

struct ConfigMapVisitor<T>(PhantomData<T>);

impl<T> ConfigMapVisitor<T> {
    fn new() -> Self {
        ConfigMapVisitor(PhantomData::default())
    }
}

impl<'de, T> Visitor<'de> for ConfigMapVisitor<T>
where
    T: TryFrom<(String, toml::Value)>,
    <T as TryFrom<(String, toml::Value)>>::Error: std::error::Error,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        write!(formatter, "valid map")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut vec = Vec::with_capacity(map.size_hint().unwrap_or_default());

        while let Some(entry) = map.next_entry::<String, toml::Value>()? {
            vec.push(
                T::try_from(entry)
                    .map_err(|e| serde::de::Error::custom(format!("Error creating enum: {e:?}")))?,
            );
        }

        Ok(vec)
    }
}

impl<'de, T> Deserialize<'de> for ConfigMap<T>
where
    T: TryFrom<(String, toml::Value)>,
    <T as TryFrom<(String, toml::Value)>>::Error: std::error::Error,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(ConfigMap(
            deserializer.deserialize_map(ConfigMapVisitor::new())?,
        ))
    }
}
