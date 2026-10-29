#[path="implementation-c182/src/source_rich_value.rs"]
mod value;
use value::*;
#[test]
fn externally_claimed_array_length_must_be_an_actual_uint32_including_positive_zero(){
    let graph=Graph{roots:vec![Atom::Reference(0)],symbols:vec![],objects:vec![Object{kind:Kind::Array,prototype:Atom::Null,properties:vec![Property{key:Key::Utf16("length".encode_utf16().collect()),enumerable:false,configurable:false,value:PropertyValue::Data{value:Atom::Binary64((-0f64).to_bits()),writable:true}}]}]};
    assert_eq!(validate_external(graph,&Capabilities::default(),Limits::default()).map(|_|()),Err(Error::InvalidArray));
}
