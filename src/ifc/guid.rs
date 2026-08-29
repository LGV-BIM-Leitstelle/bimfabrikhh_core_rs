//! IFC compressed GlobalId (22-char) from a UUID.

const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

pub fn ifc_gid(seed: &str) -> String {
    let uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, seed.as_bytes());
    compress(uuid.as_bytes())
}

fn compress(bytes: &[u8; 16]) -> String {
    let mut n: u128 = 0;
    for b in bytes {
        n = (n << 8) | u128::from(*b);
    }
    let mut out = ['0'; 22];
    for i in (0..22).rev() {
        out[i] = CHARS[(n % 64) as usize] as char;
        n /= 64;
    }
    out.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gid_len() {
        assert_eq!(ifc_gid("project").len(), 22);
    }
}
