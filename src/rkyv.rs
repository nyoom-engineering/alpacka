use std::{ops::Deref, path::PathBuf};

use rkyv::{
    string::{ArchivedString, StringResolver},
    Archive, Deserialize, Fallible, Serialize, SerializeUnsized,
};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StringPathBuf {
    inner: PathBuf,
}

impl StringPathBuf {
    #[must_use]
    pub const fn new(inner: PathBuf) -> Self {
        Self { inner }
    }
}

impl Deref for StringPathBuf {
    type Target = PathBuf;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Archive for StringPathBuf {
    type Archived = ArchivedString;
    type Resolver = StringResolver;

    unsafe fn resolve(&self, pos: usize, resolver: Self::Resolver, out: *mut Self::Archived) {
        ArchivedString::resolve_from_str(self.inner.to_str().unwrap(), pos, resolver, out);
    }
}

impl<S: Fallible + ?Sized> Serialize<S> for StringPathBuf
where
    str: SerializeUnsized<S>,
{
    fn serialize(&self, serializer: &mut S) -> Result<StringResolver, S::Error> {
        ArchivedString::serialize_from_str(self.inner.to_str().unwrap(), serializer)
    }
}

impl<D: Fallible + ?Sized> Deserialize<StringPathBuf, D> for ArchivedString {
    fn deserialize(&self, _: &mut D) -> Result<StringPathBuf, D::Error> {
        Ok(StringPathBuf {
            inner: self.as_str().to_string().into(),
        })
    }
}
