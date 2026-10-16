use core::marker::PhantomData;
#[cfg(any(feature = "manual", feature = "field-bounds"))]
use core::mem::MaybeUninit;
use wincode::SchemaRead;

#[cfg(all(feature = "manual", feature = "field-bounds"))]
compile_error!("select either manual or field-bounds, not both");

#[derive(SchemaRead)]
pub struct Leaf<'a, T> {
    pub value: T,
    pub marker: PhantomData<&'a ()>,
}

macro_rules! layer {
    ($name:ident, $child:ident) => {
        #[derive(SchemaRead)]
        pub struct $name<'a, T> {
            pub left: $child<'a, T>,
            pub right: $child<'a, T>,
        }
    };
}
layer!(Level1, Leaf);
layer!(Level2, Level1);
layer!(Level3, Level2);
layer!(Level4, Level3);
layer!(Level5, Level4);
layer!(Level6, Level5);
layer!(Level7, Level6);
layer!(Level8, Level7);
layer!(Level9, Level8);
layer!(Level10, Level9);
layer!(Level11, Level10);
layer!(Level12, Level11);
layer!(Level13, Level12);
layer!(Level14, Level13);

#[cfg(not(feature = "deep"))]
type Inner<'a, T> = Level12<'a, T>;
#[cfg(feature = "deep")]
type Inner<'a, T> = Level14<'a, T>;

#[cfg_attr(
    not(any(feature = "manual", feature = "field-bounds")),
    derive(SchemaRead)
)]
pub struct Root<'a, T> {
    pub inner: Inner<'a, T>,
}

// Both handwritten modes use exactly the same metadata and read body. Only
// the where-clause differs. The derived inner layers are identical in all modes.
#[cfg(any(feature = "manual", feature = "field-bounds"))]
macro_rules! root_reader {
    ($($bound:tt)*) => {
        // SAFETY: a one-field Rust-layout struct has its child's serialized size,
        // is not advertised as zero-copy, and is initialized by its child's reader.
        unsafe impl<'de, T, C> SchemaRead<'de, C> for Root<'de, T>
        where
            C: wincode::config::Config,
            $($bound)*
        {
            type Dst = Self;
            const TYPE_META: wincode::TypeMeta =
                <Inner<'de, T> as SchemaRead<'de, C>>::TYPE_META.keep_zero_copy(false);

            #[inline]
            fn read(reader: impl wincode::io::Reader<'de>, dst: &mut MaybeUninit<Self>) -> wincode::ReadResult<()> {
                // SAFETY: this points to the only field of uninitialized Root,
                // with exactly the same type and lifetime as the child reader.
                let inner = unsafe {
                    &mut *(&raw mut (*dst.as_mut_ptr()).inner).cast::<MaybeUninit<Inner<'de, T>>>()
                };
                <Inner<'de, T> as SchemaRead<'de, C>>::read(reader, inner)
            }
        }
    };
}

#[cfg(feature = "manual")]
root_reader!(T: SchemaRead<'de, C, Dst = T>);
#[cfg(feature = "field-bounds")]
root_reader!(Inner<'de, T>: SchemaRead<'de, C, Dst = Inner<'de, T>>);

// Essential: generic declarations alone do not reproduce the slowdown.
// `cargo check` must prove that this concrete instantiation can be decoded.
pub fn decode(bytes: &[u8]) -> wincode::ReadResult<Root<'_, f32>> {
    wincode::deserialize(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_expected_size_and_rejects_truncation() {
        let depth = if cfg!(feature = "deep") { 14 } else { 12 };
        let bytes = 1.25f32.to_le_bytes().repeat(1 << depth);
        assert!(matches!(
            <Root<'_, f32> as SchemaRead<wincode::config::DefaultConfig>>::TYPE_META,
            wincode::TypeMeta::Static { size, zero_copy: false } if size == bytes.len()
        ));
        let _: Root<'_, f32> = wincode::deserialize_exact(&bytes).unwrap();
        assert!(decode(&bytes[..bytes.len() - 1]).is_err());
    }
}
