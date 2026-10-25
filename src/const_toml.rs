//! TOMLs built into a program as consts of the types they are read into: a build script reads
//! each one with serde ([`write`]) and writes it out as the Rust expression that builds the same
//! value ([`rust`]), which [`const_toml!`](crate::const_toml) puts in a const. The types are the
//! ones in [`types`](crate::types), whose text, lists and tables a const can hold.
//!
//! In a build script (`doomify` as a build dependency):
//!
//! ```ignore
//! doomify::const_toml::write::<doomify::types::Weapon>(
//!     Path::new("doomify/weapons/colt.toml"),
//!     "doomify/weapons/colt.toml",
//! )?;
//! ```
//!
//! and in the program:
//!
//! ```ignore
//! const COLT: doomify::types::Weapon = doomify::const_toml!["doomify/weapons/colt.toml"];
//! ```

use anyhow::{Context, Result};
use serde::{
    Serialize,
    de::DeserializeOwned,
    ser::{self, Impossible},
};
use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};

/// The value a TOML in the program's build holds, as a const: the path is the one its build
/// script gave [`write`]. The value's type is the const's.
#[macro_export]
macro_rules! const_toml {
    ($path:literal) => {{
        #[allow(unused_imports)]
        use $crate::types::*;
        include!(concat!(env!("OUT_DIR"), "/const_toml/", $path, ".rs"))
    }};
}

/// Reads the TOML `file` as a `T` and writes the expression that builds it where
/// [`const_toml!`](crate::const_toml)`[name]` finds it, in the build's `OUT_DIR`. Run from a build
/// script; a TOML that isn't a `T` fails the build.
pub fn write<T: Serialize + DeserializeOwned>(file: &Path, name: &str) -> Result<PathBuf> {
    let text =
        fs::read_to_string(file).with_context(|| format!("Cannot read {}", file.display()))?;
    let value: T = toml::from_str(&text).with_context(|| format!("Invalid {}", file.display()))?;
    let out = PathBuf::from(env::var_os("OUT_DIR").context("Not run by a build script")?)
        .join("const_toml")
        .join(format!("{name}.rs"));
    fs::create_dir_all(out.parent().context("no folder")?)?;
    fs::write(&out, rust(&value)?)?;
    Ok(out)
}

/// The Rust expression that builds `value`, of a type from [`types`](crate::types) (in scope
/// where it is used).
pub fn rust<T: Serialize>(value: &T) -> Result<String> {
    value
        .serialize(Rust)
        .map_err(|Problem(problem)| anyhow::anyhow!(problem))
}

#[derive(Debug)]
pub struct Problem(String);

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Problem {}

impl ser::Error for Problem {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Problem(message.to_string())
    }
}

/// Writes a value as Rust: text as a borrowed `Cow`, lists as borrowed slices, tables as `Map`s,
/// structs by name and enums by variant (snake_case in the TOML, UpperCamelCase in Rust).
struct Rust;

const COW: &str = "::std::borrow::Cow::Borrowed";

fn camel(snake: &str) -> String {
    snake
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// The parts of a list, tuple, table or struct, joined when it ends.
struct Parts {
    open: String,
    close: &'static str,
    parts: Vec<String>,
    key: Option<String>,
}

impl Parts {
    fn new(open: impl Into<String>, close: &'static str) -> Self {
        Parts {
            open: open.into(),
            close,
            parts: Vec::new(),
            key: None,
        }
    }

    fn done(self) -> Result<String, Problem> {
        Ok(format!(
            "{}{}{}",
            self.open,
            self.parts.join(", "),
            self.close
        ))
    }
}

macro_rules! plain {
    ($($method:ident: $type:ty),*) => {
        $(fn $method(self, value: $type) -> Result<String, Problem> {
            Ok(value.to_string())
        })*
    };
}

impl ser::Serializer for Rust {
    type Ok = String;
    type Error = Problem;
    type SerializeSeq = Parts;
    type SerializeTuple = Parts;
    type SerializeTupleStruct = Impossible<String, Problem>;
    type SerializeTupleVariant = Impossible<String, Problem>;
    type SerializeMap = Parts;
    type SerializeStruct = Parts;
    type SerializeStructVariant = Impossible<String, Problem>;

    plain!(serialize_bool: bool, serialize_i8: i8, serialize_i16: i16, serialize_i32: i32,
        serialize_i64: i64, serialize_u8: u8, serialize_u16: u16, serialize_u32: u32,
        serialize_u64: u64);

    fn serialize_f32(self, value: f32) -> Result<String, Problem> {
        Ok(format!("{value:?}"))
    }

    fn serialize_f64(self, value: f64) -> Result<String, Problem> {
        Ok(format!("{value:?}"))
    }

    fn serialize_char(self, value: char) -> Result<String, Problem> {
        Ok(format!("{value:?}"))
    }

    fn serialize_str(self, value: &str) -> Result<String, Problem> {
        Ok(format!("{COW}({value:?})"))
    }

    fn serialize_bytes(self, _: &[u8]) -> Result<String, Problem> {
        Err(Problem("a TOML has no bytes".into()))
    }

    fn serialize_none(self) -> Result<String, Problem> {
        Ok("None".into())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<String, Problem> {
        Ok(format!("Some({})", value.serialize(Rust)?))
    }

    fn serialize_unit(self) -> Result<String, Problem> {
        Ok("()".into())
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<String, Problem> {
        Ok(name.into())
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<String, Problem> {
        Ok(format!("{name}::{}", camel(variant)))
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<String, Problem> {
        Ok(format!("{name}({})", value.serialize(Rust)?))
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<String, Problem> {
        Ok(format!(
            "{name}::{}({})",
            camel(variant),
            value.serialize(Rust)?
        ))
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Parts, Problem> {
        Ok(Parts::new(format!("{COW}(&["), "])"))
    }

    fn serialize_tuple(self, _: usize) -> Result<Parts, Problem> {
        Ok(Parts::new("[", "]"))
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Problem> {
        Err(Problem(format!("{name}: tuple structs aren't written")))
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Problem> {
        Err(Problem(format!("{name}: tuple variants aren't written")))
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Parts, Problem> {
        Ok(Parts::new("Map::new(&[", "])"))
    }

    fn serialize_struct(self, name: &'static str, _: usize) -> Result<Parts, Problem> {
        Ok(Parts::new(format!("{name} {{ "), " }"))
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Problem> {
        Err(Problem(format!("{name}: struct variants aren't written")))
    }
}

impl ser::SerializeSeq for Parts {
    type Ok = String;
    type Error = Problem;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Problem> {
        self.parts.push(value.serialize(Rust)?);
        Ok(())
    }

    fn end(self) -> Result<String, Problem> {
        self.done()
    }
}

impl ser::SerializeTuple for Parts {
    type Ok = String;
    type Error = Problem;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Problem> {
        self.parts.push(value.serialize(Rust)?);
        Ok(())
    }

    fn end(self) -> Result<String, Problem> {
        self.done()
    }
}

impl ser::SerializeMap for Parts {
    type Ok = String;
    type Error = Problem;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Problem> {
        self.key = Some(key.serialize(Rust)?);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Problem> {
        let key = self
            .key
            .take()
            .ok_or_else(|| Problem("a value without a key".into()))?;
        self.parts
            .push(format!("({key}, {})", value.serialize(Rust)?));
        Ok(())
    }

    fn end(self) -> Result<String, Problem> {
        self.done()
    }
}

impl ser::SerializeStruct for Parts {
    type Ok = String;
    type Error = Problem;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        name: &'static str,
        value: &T,
    ) -> Result<(), Problem> {
        self.parts
            .push(format!("{name}: {}", value.serialize(Rust)?));
        Ok(())
    }

    fn end(self) -> Result<String, Problem> {
        self.done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::borrow::Cow;

    const SET: SoundSet = SoundSet {
        name: Cow::Borrowed("Zombies"),
        sounds: Map::new(&[(
            Cow::Borrowed("zombie/die"),
            Sound {
                alias: Cow::Borrowed("zmb_death"),
                note: Cow::Borrowed(""),
            },
        )]),
    };

    #[test]
    fn a_toml_is_written_as_the_rust_that_builds_it() {
        let set: SoundSet = toml::from_str(
            "name = \"Zombies\"\n[sounds]\n\"zombie/die\" = { alias = \"zmb_death\" }\n",
        )
        .unwrap();
        assert_eq!(rust(&set).unwrap(), rust(&SET).unwrap());
        assert_eq!(
            rust(&SET).unwrap(),
            "SoundSet { name: ::std::borrow::Cow::Borrowed(\"Zombies\"), sounds: Map::new(&[(::std::borrow::Cow::Borrowed(\"zombie/die\"), Sound { alias: ::std::borrow::Cow::Borrowed(\"zmb_death\"), note: ::std::borrow::Cow::Borrowed(\"\") })]) }"
        );
        let text =
            fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/weapons/colt.toml")).unwrap();
        let weapon: Weapon = toml::from_str(&text).unwrap();
        let written = rust(&weapon).unwrap();
        assert!(written.contains("kind: Kind::Bullet"));
        assert!(written.contains("animations: Map::new(&[("));
        assert!(written.contains("hip: None") || written.contains("hip: Some(["));
        assert_eq!(camel("quick_revive"), "QuickRevive");
    }
}
