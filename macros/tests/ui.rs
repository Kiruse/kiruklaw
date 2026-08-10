#[test]
fn ui() {
  let t = trybuild::TestCases::new();
  t.compile_fail("tests/ui/self_receiver.rs");
  t.compile_fail("tests/ui/unsupported_type.rs");
}
