


#[derive(strum::FromRepr, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum TypeCode {
    Null = 42,
    Unknown = 0,
    Boolean = 111,
    Byte = 98,
    Short = 107,
    Integer = 105,
    Long = 108,
    Double = 100,
    Float = 102,
    String = 115,
    Custom = 99,
    Hashtable = 104,
    Dictionary = 68,
    Array = 121,
    ObjectArray = 122,
    StringArray = 97,
    IntegerArray = 110,
    ByteArray = 120,
    EventData = 101,
    OperationResponse = 112,
    OperationRequest = 113,
}
