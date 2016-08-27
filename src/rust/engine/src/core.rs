use std::fmt;

// The type of a python object (which itself has a type, but which is not
// represented by a Key, because that would result in a recursive structure.)
pub type TypeId = Digest;

// An identifier for a python function.
pub type Function = Digest;

// The name of a field.
// TODO: Change to just a Digest... we don't need type information here.
pub type Field = Key;

// On the python side this is string->string; but to allow for equality checks
// without a roundtrip to python, we keep them encoded here.
pub type Variants = Vec<(Field, Field)>;

// NB: These structs are fairly small, so we allow copying them by default.
#[repr(C)]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct Digest {
  digest: [u8;32],
}

impl fmt::Debug for Digest {
  fn fmt(&self, fmtr: &mut fmt::Formatter) -> Result<(), fmt::Error> {
    for byte in self.digest.iter().take(4) {
      try!(fmtr.write_fmt(format_args!("{:02x}", byte)));
    }
    Ok(())
  }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Key {
  digest: Digest,
  type_id: TypeId,
}

impl Key {
  pub fn empty() -> Key {
    Key {
      digest: Digest {
        digest: [0;32]
      },
      type_id: TypeId {
        digest: [0;32]
      },
    }
  }

  pub fn digest(&self) -> &Digest {
    &self.digest
  }

  pub fn type_id(&self) -> &TypeId {
    &self.type_id
  }
}