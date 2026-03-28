use shamrocq::Vm;

fn make_vm(buf: &mut [u8]) -> Vm {
    Vm::new(buf)
}

#[test]
fn value_bytes_roundtrip() {
    let mut buf = vec![0u8; 4096];
    let mut vm = make_vm(&mut buf);

    let val = vm.arena.alloc_bytes(b"hello").unwrap();
    assert!(val.is_bytes());
    assert!(!val.is_ctor());
    assert!(!val.is_integer());
    assert!(!val.is_callable());
    assert_eq!(val.bytes_len(), 5);
    assert_eq!(vm.arena.bytes_data(val), b"hello");
}

#[test]
fn bytes_empty() {
    let mut buf = vec![0u8; 4096];
    let mut vm = make_vm(&mut buf);

    let val = vm.arena.alloc_bytes(b"").unwrap();
    assert!(val.is_bytes());
    assert_eq!(val.bytes_len(), 0);
    assert_eq!(vm.arena.bytes_data(val), b"");
}

#[test]
fn bytes_non_aligned_lengths() {
    let mut buf = vec![0u8; 4096];
    let mut vm = make_vm(&mut buf);

    for len in [1, 2, 3, 4, 5, 6, 7, 8] {
        let data: Vec<u8> = (0..len).map(|i| i as u8).collect();
        let val = vm.arena.alloc_bytes(&data).unwrap();
        assert_eq!(val.bytes_len(), len);
        assert_eq!(vm.arena.bytes_data(val), &data[..]);
    }
}

#[test]
fn bytes_max_length() {
    let mut buf = vec![0u8; 65536];
    let mut vm = make_vm(&mut buf);

    let data = [0xABu8; 255];
    let val = vm.arena.alloc_bytes(&data).unwrap();
    assert_eq!(val.bytes_len(), 255);
    assert_eq!(vm.arena.bytes_data(val), &data[..]);
}

#[test]
fn bytes_concat_basic() {
    let mut buf = vec![0u8; 4096];
    let mut vm = make_vm(&mut buf);

    let a = vm.arena.alloc_bytes(b"foo").unwrap();
    let b = vm.arena.alloc_bytes(b"bar").unwrap();
    let c = vm.arena.bytes_concat(a, b).unwrap();
    assert_eq!(c.bytes_len(), 6);
    assert_eq!(vm.arena.bytes_data(c), b"foobar");
}

#[test]
fn bytes_concat_empty() {
    let mut buf = vec![0u8; 4096];
    let mut vm = make_vm(&mut buf);

    let a = vm.arena.alloc_bytes(b"hello").unwrap();
    let b = vm.arena.alloc_bytes(b"").unwrap();

    let c = vm.arena.bytes_concat(a, b).unwrap();
    assert_eq!(vm.arena.bytes_data(c), b"hello");

    let d = vm.arena.bytes_concat(b, a).unwrap();
    assert_eq!(vm.arena.bytes_data(d), b"hello");
}

#[test]
fn bytes_multiple_no_corruption() {
    let mut buf = vec![0u8; 4096];
    let mut vm = make_vm(&mut buf);

    let a = vm.arena.alloc_bytes(b"abc").unwrap();
    let b = vm.arena.alloc_bytes(b"defgh").unwrap();
    let c = vm.arena.alloc_bytes(b"i").unwrap();

    assert_eq!(vm.arena.bytes_data(a), b"abc");
    assert_eq!(vm.arena.bytes_data(b), b"defgh");
    assert_eq!(vm.arena.bytes_data(c), b"i");
}
