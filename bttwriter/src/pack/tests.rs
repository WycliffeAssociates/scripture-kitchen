use super::*;

#[test]
fn ids_are_gits() {
    // `git hash-object -t blob /dev/null` and `git hash-object -t tree /dev/null`.
    assert_eq!(
        ObjectId::of(Kind::Blob, b"").to_string(),
        "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
    );
    assert_eq!(
        ObjectId::of(Kind::Tree, b"").to_string(),
        "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
    );
    // `printf 'hello\n' | git hash-object --stdin`
    assert_eq!(
        ObjectId::of(Kind::Blob, b"hello\n").to_string(),
        "ce013625030ba8dba906f756967f9e9ca394464a"
    );
    let id = ObjectId::of(Kind::Blob, b"x");
    assert_eq!(ObjectId::from_hex(&id.to_string()), Some(id));
    assert_eq!(ObjectId::from_hex("zz"), None);
}

#[test]
fn crc_is_the_standard_crc32() {
    assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn zones_print_as_git_writes_them() {
    let at = |offset_minutes| Signature {
        name: "Mwila Chanda".into(),
        email: "mwila@example.org".into(),
        at: 1_782_000_000,
        offset_minutes,
    };
    assert_eq!(
        at(120).to_string(),
        "Mwila Chanda <mwila@example.org> 1782000000 +0200"
    );
    assert_eq!(at(-300).to_string().rsplit(' ').next(), Some("-0500"));
    assert_eq!(at(330).to_string().rsplit(' ').next(), Some("+0530"));
    assert_eq!(at(0).to_string().rsplit(' ').next(), Some("+0000"));
}

fn read_offset(bytes: &[u8]) -> (u64, usize) {
    let mut at = 0;
    let mut c = bytes[at];
    at += 1;
    let mut value = u64::from(c & 0x7f);
    while c & 0x80 != 0 {
        c = bytes[at];
        at += 1;
        value = ((value + 1) << 7) | u64::from(c & 0x7f);
    }
    (value, at)
}

fn read_size(bytes: &[u8], at: &mut usize) -> u64 {
    let mut value = 0;
    let mut shift = 0;
    loop {
        let c = bytes[*at];
        *at += 1;
        value |= u64::from(c & 0x7f) << shift;
        shift += 7;
        if c & 0x80 == 0 {
            return value;
        }
    }
}

#[test]
fn offsets_and_sizes_read_back() {
    for value in [0, 1, 127, 128, 129, 16_383, 16_384, 16_511, 16_512, 1 << 40] {
        let mut out = Vec::new();
        offset_varint(value, &mut out);
        assert_eq!(read_offset(&out), (value, out.len()), "{value}");
        let mut out = Vec::new();
        size_varint(value, &mut out);
        assert_eq!(read_size(&out, &mut 0), value, "{value}");
    }
    // Git's examples: 128 is two bytes, 0x80 0x00.
    let mut out = Vec::new();
    offset_varint(128, &mut out);
    assert_eq!(out, [0x80, 0x00]);
}

#[test]
fn entry_headers_carry_kind_and_size() {
    for (kind, size) in [(3u8, 0u64), (1, 15), (2, 16), (3, 1_000_000), (6, 70_000)] {
        let mut out = Vec::new();
        entry_header(kind, size, &mut out);
        assert_eq!((out[0] >> 4) & 0x07, kind);
        let mut value = u64::from(out[0] & 0x0f);
        let mut shift = 4;
        for byte in &out[1..] {
            value |= u64::from(byte & 0x7f) << shift;
            shift += 7;
        }
        assert_eq!(value, size);
        assert!(out[..out.len() - 1].iter().all(|b| b & 0x80 != 0));
        assert_eq!(out[out.len() - 1] & 0x80, 0);
    }
}

/// Git's delta application, for checking ours.
fn apply(base: &[u8], delta: &[u8]) -> Vec<u8> {
    let mut at = 0;
    assert_eq!(read_size(delta, &mut at), base.len() as u64);
    let size = read_size(delta, &mut at) as usize;
    let mut out = Vec::with_capacity(size);
    while at < delta.len() {
        let op = delta[at];
        at += 1;
        if op & 0x80 != 0 {
            let mut offset = 0usize;
            let mut length = 0usize;
            for i in 0..4 {
                if op & (1 << i) != 0 {
                    offset |= usize::from(delta[at]) << (8 * i);
                    at += 1;
                }
            }
            for i in 0..3 {
                if op & (1 << (4 + i)) != 0 {
                    length |= usize::from(delta[at]) << (8 * i);
                    at += 1;
                }
            }
            if length == 0 {
                length = 0x10000;
            }
            out.extend_from_slice(&base[offset..offset + length]);
        } else {
            assert!(op > 0, "opcode 0 is reserved");
            out.extend_from_slice(&delta[at..at + usize::from(op)]);
            at += usize::from(op);
        }
    }
    assert_eq!(out.len(), size);
    out
}

#[test]
fn deltas_rebuild_their_target() {
    let book = |marker: &str| {
        (1..=2_000)
            .map(|v| format!("\\v {v} Mu kale {marker} ŋaŋa ẽ text text text\n"))
            .collect::<String>()
    };
    let base = book("a").into_bytes();
    let cases: Vec<Vec<u8>> = vec![
        base.clone(),
        book("b").into_bytes(),
        book("a")
            .replace("\\v 1000 ", "\\v 1000 inserted ")
            .into_bytes(),
        format!("prefix {}", book("a")).into_bytes(),
        format!("{} suffix", book("a")).into_bytes(),
        Vec::new(),
        b"short".to_vec(),
        vec![0u8; 300_000],
    ];
    for target in &cases {
        let delta = delta_of(&base, target);
        assert_eq!(&apply(&base, &delta), target);
    }
    // A one-verse change costs bytes, not the book: >64 KiB copies split.
    let edit = book("a").replace("\\v 1000 ", "\\v 1000 x ").into_bytes();
    assert!(base.len() > 0x10000);
    assert!(delta_of(&base, &edit).len() < 64);
}

#[test]
fn a_lane_deltas_until_its_chain_is_long() {
    let mut pack = PackWriter::new();
    let mut text = "\\id JUD\n".repeat(400);
    for n in 0..120 {
        text.push_str(&format!("\\v {n} save\n"));
        pack.add(Kind::Blob, text.as_bytes(), Some("66-JUD.usfm"));
    }
    let stats = pack.finish().stats;
    assert_eq!(stats.objects, 120);
    // One whole object per chain of 51: 120 saves → 3 whole, 117 deltas.
    assert_eq!(stats.deltas, 117);
}

#[test]
fn adding_an_object_twice_stores_it_once() {
    let mut pack = PackWriter::new();
    let a = pack.add(Kind::Blob, b"same", None);
    let b = pack.add(Kind::Blob, b"same", Some("lane"));
    assert_eq!(a, b);
    assert_eq!(pack.finish().stats.objects, 1);
}

#[test]
fn trees_order_entries_by_bytes() {
    let mut files = BTreeMap::new();
    let id = ObjectId::of(Kind::Blob, b"");
    files.insert("66-JUD.usfm".to_string(), id);
    files.insert("01-GEN.usfm".to_string(), id);
    let body = tree_body(&files);
    assert!(body.starts_with(b"100644 01-GEN.usfm\0"));
    assert_eq!(body.len(), 2 * ("100644 01-GEN.usfm\0".len() + 20));
}
