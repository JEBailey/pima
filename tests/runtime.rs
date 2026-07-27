use pima::runtime::{PersistentList, Value};

#[test]
fn persistent_lists_preserve_the_original_when_pushed() {
    let original: PersistentList = [Value::Integer(1), Value::Integer(2)].into_iter().collect();
    let extended = original.push_front(Value::Integer(0));

    assert_eq!(original.len(), 2);
    assert!(matches!(original.first(), Some(Value::Integer(1))));
    assert_eq!(extended.len(), 3);
    assert!(matches!(extended.first(), Some(Value::Integer(0))));
}

#[test]
fn persistent_list_rest_returns_a_shared_tail_value() {
    let list: PersistentList = [Value::Integer(1), Value::Integer(2)].into_iter().collect();
    let rest = list.rest().expect("non-empty list has a rest");

    assert_eq!(rest.len(), 1);
    assert!(matches!(rest.first(), Some(Value::Integer(2))));
    assert!(PersistentList::empty().rest().is_none());
}
