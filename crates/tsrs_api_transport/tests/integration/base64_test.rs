#[test]
fn rfc4648_vectors() {
    let cases = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];
    for (input, expected) in cases {
        assert_eq!(tsrs_api_transport::base64::encode(input.as_bytes()), expected);
    }
}
