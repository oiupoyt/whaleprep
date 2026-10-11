use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Repeated {
    chapters: Vec<String>,
}

#[test]
fn repeated_keys_do_not_decode_into_vec() {
    // Documents the behaviour that forced comma-separated ids on the build form.
    let out = serde_urlencoded::from_str::<Repeated>("chapters=1&chapters=4");
    assert!(out.is_err(), "serde_urlencoded keeps the last repeated key only");
}

#[test]
fn comma_separated_ids_decode() {
    #[derive(Deserialize, Debug)]
    struct Cs {
        chapters: String,
    }
    let v = serde_urlencoded::from_str::<Cs>("chapters=1,4,7").expect("decodes");
    assert_eq!(v.chapters, "1,4,7");
}