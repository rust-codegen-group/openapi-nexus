//! Regression coverage for the Python layout behavior required from sigil-stitch.
//!
//! These assertions intentionally exercise the semantic sigil-stitch API rather
//! than a post-render formatter. A dependency downgrade must not silently bring
//! back spaces around keyword arguments or unindented generic continuations.

use sigil_stitch::assert_quote;
use sigil_stitch::lang::python::Python;
use sigil_stitch::prelude::*;

#[test]
fn python_keyword_arguments_have_no_spaces_around_equals() {
    assert_quote!(Python, { field(kw_only = True) }, "field(kw_only=True)\n");
}

#[test]
fn ordinary_python_assignment_keeps_spaces() {
    assert_quote!(Python, { value = True }, "value = True\n");
}

#[test]
fn narrow_python_generic_has_continuation_indentation() {
    let ty = TypeName::application(
        TypeName::primitive("dict"),
        vec![
            TypeArgument::Single(TypeName::primitive("str")),
            TypeArgument::Single(TypeName::primitive("int")),
        ],
    );
    let model = TypeSpec::builder("Model", TypeKind::Class)
        .add_field(FieldSpec::builder("value", ty).build().unwrap())
        .build()
        .unwrap();
    let file = FileSpec::builder_with("model.py", Python::new())
        .add_type(model)
        .build()
        .unwrap();

    assert_eq!(
        file.render(100).unwrap(),
        "class Model:\n    value: dict[str, int]\n",
    );
    assert_eq!(
        file.render(20).unwrap(),
        "class Model:\n    value: dict[\n        str, int\n    ]\n",
    );
}
