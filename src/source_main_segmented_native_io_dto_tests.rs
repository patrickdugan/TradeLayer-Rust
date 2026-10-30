use super::*;
#[test]
fn actual_custom_and_real_filesystem_error_roundtrip_as_typed_io() {
    let custom = std::io::Error::other("actual custom native error shape");
    let data = ActualValidityIoDataV1::capture(&custom).unwrap();
    let value = data.value();
    let parsed = ActualValidityIoDataV1::from_value(&value).unwrap();
    assert_eq!(format!("{:?}", parsed.reconstruct()), format!("{custom:?}"));
    let parent = std::env::var_os("NATIVE_FACT_DTO_TMP")
        .or_else(|| std::env::var_os("MAIN_SERVICE_TMP"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&parent).unwrap();
    let path = parent.join(format!("does-not-exist-{}", std::process::id()));
    assert!(!path.exists());
    let error = std::fs::File::open(path).err().unwrap();
    let data = ActualValidityIoDataV1::capture(&error).unwrap();
    let parsed = ActualValidityIoDataV1::from_value(&data.value()).unwrap();
    assert_eq!(parsed.reconstruct().raw_os_error(), error.raw_os_error());
    assert_eq!(format!("{:?}", parsed.reconstruct()), format!("{error:?}"));
}
#[test]
fn unknown_shape_error_text_and_changed_os_message_cannot_become_a_typed_io_fact() {
    let error = std::io::Error::other("data");
    let original = ActualValidityIoDataV1::capture(&error).unwrap().value();
    let mut bad = original.clone();
    bad.insert("StorageStopped", s("supplied source return text"))
        .unwrap();
    assert!(ActualValidityIoDataV1::from_value(&bad).is_err());
    let mut bad = original;
    bad.insert("kind", s("StructuralOrResourceRejection"))
        .unwrap();
    assert!(ActualValidityIoDataV1::from_value(&bad).is_err());
    let error = std::io::Error::from_raw_os_error(2);
    let mut bad = ActualValidityIoDataV1::capture(&error).unwrap().value();
    bad.insert("message", s("arbitrary source failure display"))
        .unwrap();
    assert!(ActualValidityIoDataV1::from_value(&bad).is_err());
}
#[test]
fn non_string_custom_inner_error_is_not_silently_replaced_by_a_display_string() {
    #[derive(Debug)]
    struct ForeignShape;
    impl std::fmt::Display for ForeignShape {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a display is not the typed Debug shape")
        }
    }
    impl std::error::Error for ForeignShape {}
    let error = std::io::Error::other(ForeignShape);
    assert!(ActualValidityIoDataV1::capture(&error).is_err());
    assert!(error.get_ref().unwrap().is::<ForeignShape>());
}

#[test]
fn os_kind_is_derived_from_actual_code_even_when_no_public_enum_name_exists() {
    for code in [1117, i32::MIN] {
        let actual = std::io::Error::from_raw_os_error(code);
        let data = ActualValidityIoDataV1::capture(&actual).unwrap();
        let parsed = ActualValidityIoDataV1::from_value(&data.value()).unwrap();
        assert_eq!(format!("{:?}", parsed.reconstruct()), format!("{actual:?}"));
    }
}
