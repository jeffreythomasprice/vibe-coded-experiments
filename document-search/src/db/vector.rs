/// Convert a `&[f32]` to a little-endian byte vector suitable for storage in
/// a turso `F32_BLOB(N)` column. Pairs with [`from_blob`].
pub fn to_blob(vec: &[f32]) -> Vec<u8> {
    bytemuck::cast_slice(vec).to_vec()
}

/// Reinterpret a byte slice as `&[f32]`. Errors if the byte length isn't a
/// multiple of 4 (i.e. didn't come from a properly-stored embedding).
pub fn from_blob(bytes: &[u8]) -> Result<&[f32], bytemuck::PodCastError> {
    bytemuck::try_cast_slice(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let v = vec![1.0_f32, -2.5, 0.0, std::f32::consts::PI];
        let blob = to_blob(&v);
        assert_eq!(blob.len(), v.len() * 4);
        let back = from_blob(&blob).unwrap();
        assert_eq!(back, v.as_slice());
    }
}
