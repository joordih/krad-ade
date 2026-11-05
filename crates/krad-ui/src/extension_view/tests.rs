use super::form::{field_value, LocalValue};
use super::*;
use krad_extensions::api::view::{
    Align, Direction, Field, FieldValue, InputKind, Justify, Style, Wrap,
};
use krad_extensions::api::{ActionId, IdError};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn key(text: &str) -> Result<NodeKey, IdError> {
    text.parse()
}

fn action(text: &str) -> Result<ActionId, IdError> {
    text.parse()
}

fn field(name: &str, kind: FieldKind) -> Result<Field, IdError> {
    Ok(Field {
        key: key(name)?,
        label: name.to_owned(),
        kind,
    })
}

#[test]
fn every_input_and_text_field_gets_an_input_slot() -> TestResult {
    let filter = Node {
        key: key("filter")?,
        element: Element::Input {
            action: action("filter")?,
            value: String::new(),
            placeholder: None,
            kind: InputKind::Secret,
        },
    };
    let fields = vec![
        field(
            "image",
            FieldKind::Text {
                value: String::new(),
                placeholder: None,
                kind: InputKind::Text,
            },
        )?,
        field("port", FieldKind::Number { value: 8080.0 })?,
        field("detach", FieldKind::Toggle { value: false })?,
    ];
    let run = Node {
        key: key("run")?,
        element: Element::Form {
            submit: action("run")?,
            fields,
            submit_label: None,
        },
    };
    let root = Node {
        key: key("root")?,
        element: Element::Flex {
            direction: Direction::default(),
            align: Align::default(),
            justify: Justify::default(),
            wrap: Wrap::default(),
            style: Style::default(),
            children: vec![filter, run],
        },
    };
    let mut wanted = Vec::new();
    collect_inputs(&root, &mut wanted);
    let slots: Vec<_> = wanted.iter().map(|(slot, _)| slot.clone()).collect();
    assert_eq!(slots.len(), 3);
    assert_eq!(slots[0], InputSlot::Node(key("filter")?));
    assert!(wanted[0].1.masked);
    assert_eq!(wanted[2].1.value, "8080");
    Ok(())
}

#[test]
fn form_values_take_what_was_typed_or_chosen_over_the_initial_value() {
    let number = FieldKind::Number { value: 1.0 };
    assert_eq!(
        field_value(&number, Some("42".to_owned()), None),
        FieldValue::Number(42.0)
    );
    assert_eq!(
        field_value(&number, Some("x".to_owned()), None),
        FieldValue::Number(1.0)
    );
    let toggle = FieldKind::Toggle { value: false };
    assert_eq!(
        field_value(&toggle, None, Some(&LocalValue::Toggle(true))),
        FieldValue::Toggle(true)
    );
    let text = FieldKind::Text {
        value: "nginx".to_owned(),
        placeholder: None,
        kind: InputKind::Text,
    };
    assert_eq!(
        field_value(&text, None, None),
        FieldValue::Text("nginx".to_owned())
    );
}
