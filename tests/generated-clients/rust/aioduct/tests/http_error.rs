use typed_error_responses_api::runtime::error::ApiCallError;

#[test]
fn api_call_error_is_pointer_sized() {
    assert_eq!(
        std::mem::size_of::<ApiCallError>(),
        std::mem::size_of::<Box<()>>()
    );
}
