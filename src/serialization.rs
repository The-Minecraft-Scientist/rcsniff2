use core::slice;
use std::{
    backtrace::Backtrace,
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    io::{Cursor, Read},
};

use anyhow::{Context, Result};
use enum_dispatch::enum_dispatch;
use newtype::NewType;
use strum::AsRefStr;

use crate::util::{HashableDouble, HashableFloat, HashableHashmap};

use self::{op_code::WebServicesOpCode, type_code::TypeCode};

pub mod op_code;
pub mod type_code;
#[enum_dispatch]
//We only use enum dispatch for its automatic From impl for al variant inner types
trait ValueDummyTrait {}

#[enum_dispatch(ValueDummyTrait)]
#[derive(Clone, Hash, PartialEq, Eq, AsRefStr)]
//This is currently a bit jank. the HashableX stuff needs to exist because this format isnt well specified and I wouldn't be surprised if there's a hashtable or dict with hashtables as a key type..
pub enum Value {
    Byte(u8),
    Bool(bool),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(HashableFloat),
    Double(HashableDouble),
    String(String),
    Array(Vec<Value>),
    ObjectArray(ObjectArray),
    StringArray(Vec<String>),
    IntegerArray(Vec<i32>),
    ByteArray(Vec<u8>),
    ///Dict has fixed key and value type
    Dictionary(HashableHashmap<Value, Value>),
    // Any type allowed for key/value
    HashTable(HashTable<Value, Value>),
    EventData(EventData),
    OperationRequest(OperationRequest),
    OperationResponse(OperationResponse),
    Null(()),
}
impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Byte(b) => f.write_fmt(format_args!("{}({})", self.as_ref(), b)),
            Value::Bool(b) => f.write_fmt(format_args!("{}({})", self.as_ref(), b)),
            Value::Short(b) => f.write_fmt(format_args!("{}({})", self.as_ref(), b)),
            Value::Int(b) => f.write_fmt(format_args!("{}({})", self.as_ref(), b)),
            Value::Long(b) => f.write_fmt(format_args!("{}({})", self.as_ref(), b)),
            Value::Double(b) => f.write_fmt(format_args!("{}({})", self.as_ref(), b.0)),
            Value::Float(b) => f.write_fmt(format_args!("{}({})", self.as_ref(), b.0)),
            Value::String(b) => f.write_fmt(format_args!("\"{}\"", b)),
            Value::StringArray(b) => b.fmt(f),
            Value::Array(b) => b.fmt(f),
            Value::IntegerArray(b) => b.fmt(f),
            Value::ByteArray(b) => f.write_fmt(format_args!("{:x?}", b)),
            Value::EventData(b) => b.fmt(f),
            Value::OperationRequest(b) => b.fmt(f),
            Value::OperationResponse(b) => b.fmt(f),
            Value::ObjectArray(b) => b.0.fmt(f),
            Value::Dictionary(b) => b.0.fmt(f),
            Value::HashTable(b) => b.0 .0.fmt(f),
            Value::Null(_) => f.write_str("null"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventData {
    params: ParameterTable,
    event_code: u8,
}
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct OperationResponse {
    opcode: u8,
    return_code: i16,
    debug_message: Box<Value>,
    parameters: ParameterTable,
}
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct OperationRequest {
    opcode: u8,
    parameters: ParameterTable,
}
impl Debug for OperationResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("OperationResponse");
        if let Some(code) = WebServicesOpCode::from_repr(self.opcode) {
            s.field("opcode", &code);
        } else {
            s.field("opcode", &self.opcode);
        }
        s.field("parameters", &self.parameters).finish()
    }
}
impl Debug for OperationRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("OperationRequest");
        if let Some(code) = WebServicesOpCode::from_repr(self.opcode) {
            s.field("opcode", &code);
        } else {
            s.field("opcode", &self.opcode);
        }
        s.field("parameters", &self.parameters).finish()
    }
}
#[derive(Clone, NewType, PartialEq, Eq, Hash)]
pub struct ParameterTable(HashableHashmap<u8, Value>);
impl Debug for ParameterTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Parameter Table").field(&self.0 .0).finish()
    }
}

#[derive(Debug, Clone, NewType, PartialEq, Eq, Hash)]
pub struct HashTable<K: Hash + Eq, V: Hash + Eq>(HashableHashmap<K, V>);

#[derive(Debug, Clone, NewType, PartialEq, Eq, Hash)]
pub struct ObjectArray(Vec<Value>);

#[derive(Debug, Clone, Copy)]
pub struct CustomType {
    serialize: for<'a> fn(&'a [u8]) -> Value,
    deserialize: for<'a> fn(&'a [u8]) -> Value,
}
pub struct StreamDeserializer<'a> {
    pub reader: Cursor<&'a [u8]>,
    pub custom_type_impls: HashMap<u8, CustomType>,
}
impl<'a> StreamDeserializer<'a> {
    pub fn new(reader: Cursor<&'a [u8]>) -> Self {
        Self {
            reader,
            custom_type_impls: HashMap::new(),
        }
    }
    pub fn read_byte(&mut self) -> Result<u8> {
        let mut val = 0u8;
        self.reader.read_exact(slice::from_mut(&mut val))?;
        Ok(val)
    }
    pub fn read_bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut buf = [0u8; N];
        self.reader.read_exact(&mut buf)?;
        Ok(buf)
    }
    pub fn deserialize(&mut self, type_code: u8) -> Result<Value> {
        let Ok(code) = TypeCode::from_repr(type_code).context("getting type code") else {
            return Ok(Value::Null(()));
        };
        Ok(match code {
            TypeCode::Unknown | TypeCode::Null => Value::Null(()),
            TypeCode::Boolean => self.deserialize_bool()?.into(),
            TypeCode::Byte => self.deserialize_byte()?.into(),
            TypeCode::Short => self.deserialize_short()?.into(),
            TypeCode::Integer => self.deserialize_int()?.into(),
            TypeCode::Long => self.deserialize_long()?.into(),
            TypeCode::Float => self.deserialize_float()?.into(),
            TypeCode::Double => self.deserialize_double()?.into(),
            TypeCode::String => self.deserialize_string()?.into(),
            TypeCode::Array => self.deserialize_array()?.into(),
            TypeCode::Custom => {
                let custom_code = self.read_byte()?;
                self.deserialize_custom(custom_code)?
            }
            TypeCode::Hashtable => self.deserialize_hashtable()?.into(),
            TypeCode::Dictionary => self.deserialize_dictionary()?.into(),
            TypeCode::ObjectArray => self.deserialize_object_array()?.into(),
            TypeCode::StringArray => self.deserialize_string_array()?.into(),
            TypeCode::IntegerArray => self.deserialize_int_array()?.into(),
            TypeCode::ByteArray => self.deserialize_byte_array()?.into(),
            TypeCode::OperationRequest => self.deserialize_operation_request()?.into(),
            TypeCode::OperationResponse => self.deserialize_operation_response()?.into(),
            TypeCode::EventData => self.deserialize_event_data()?.into(),
        })
    }
    pub fn deserialize_byte(&mut self) -> Result<u8> {
        Ok(self.read_byte()?)
    }
    pub fn deserialize_bool(&mut self) -> Result<bool> {
        Ok(self.read_byte()? != 0)
    }
    pub fn deserialize_short(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.read_bytes()?))
    }
    pub fn deserialize_int(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.read_bytes()?))
    }
    pub fn deserialize_long(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.read_bytes()?))
    }
    pub fn deserialize_float(&mut self) -> Result<HashableFloat> {
        Ok(f32::from_be_bytes(self.read_bytes()?).into())
    }
    pub fn deserialize_double(&mut self) -> Result<HashableDouble> {
        Ok(f64::from_be_bytes(self.read_bytes()?).into())
    }
    pub fn deserialize_string(&mut self) -> Result<String> {
        let len = self.deserialize_short()?;
        let mut v = vec![0; len as usize];
        self.reader.read_exact(&mut v)?;
        Ok(String::from_utf8(v)?)
    }
    pub fn read_type_code(&mut self) -> Result<TypeCode> {
        let val = self.read_byte()?;
        let ret = TypeCode::from_repr(val)
            .context(format!("parsing type code, got unexpected code {}", val));
        if ret.is_err() {
            #[cfg(debug_assertions)]
            println!("{:#?}", Backtrace::force_capture());
        }
        ret
    }
    pub fn deserialize_array(&mut self) -> Result<Vec<Value>> {
        let len = self.deserialize_short()?;
        let mut v = Vec::with_capacity(len as usize);
        let item_type = self.read_type_code()?;
        match item_type {
            TypeCode::Array => {
                for _ in 0..len {
                    v.push(self.deserialize_array()?.into())
                }
            }
            TypeCode::ByteArray => {
                for _ in 0..len {
                    v.push(self.deserialize_byte_array()?.into())
                }
            }
            TypeCode::Custom => {
                let custom_type = self.read_byte()?;
                let custom_type = *self.custom_type_impls.get(&custom_type).context(format!(
                    "failed to get deserializer for custom type id {}",
                    custom_type
                ))?;
                let mut buf = Vec::new();
                for _ in 0..len {
                    let len2 = self.deserialize_short()?;
                    buf.clear();
                    buf.resize(len2 as usize, 0u8);
                    self.reader.read_exact(&mut buf)?;
                    v.push((custom_type.deserialize)(&buf));
                }
            }
            TypeCode::Dictionary => self.deserialize_dict_array(len, &mut v)?,
            _ => {
                for _ in 0..len {
                    v.push(self.deserialize(item_type as u8)?);
                }
            }
        }
        Ok(v)
    }
    pub fn deserialize_custom(&mut self, custom_type: u8) -> Result<Value> {
        let len = self.deserialize_short()?;
        let custom_type = *self.custom_type_impls.get(&custom_type).context(format!(
            "failed to get deserializer for custom type id {}",
            custom_type
        ))?;
        let mut buf = vec![0; len as usize];
        self.reader.read_exact(&mut buf)?;

        Ok((custom_type.deserialize)(&buf))
    }
    pub fn deserialize_dictionary_type(&mut self) -> Result<(TypeCode, TypeCode)> {
        let arr = self.read_bytes::<2>()?;
        Ok((
            TypeCode::from_repr(arr[0]).context("failed to get dictionary key type code")?,
            TypeCode::from_repr(arr[1]).context("failed to get dictionary key type code")?,
        ))
    }
    pub fn deserialize_dict_array(&mut self, size: i16, values: &mut Vec<Value>) -> Result<()> {
        let (key_type, value_type) = self.deserialize_dictionary_type()?;

        for _ in 0..size {
            let dictlen = self.deserialize_short()?;
            let mut m = HashMap::with_capacity(size as usize);
            for _ in 0..dictlen {
                let key = self.deserialize_maybe_typed(key_type)?;
                let value = self.deserialize_maybe_typed(value_type)?;
                m.insert(key, value);
            }
            values.push(HashableHashmap(m).into());
        }
        Ok(())
    }
    pub fn deserialize_maybe_typed(&mut self, code: TypeCode) -> Result<Value> {
        let mut code = code;
        if code == TypeCode::Unknown {
            code = self.read_type_code()?;
        }
        self.deserialize(code as u8)
    }
    pub fn deserialize_byte_array(&mut self) -> Result<Vec<u8>> {
        let len = self.deserialize_int()?;
        let mut v = vec![0; len as usize];
        self.reader.read_exact(&mut v)?;
        Ok(v)
    }
    pub fn deserialize_int_array(&mut self) -> Result<Vec<i32>> {
        let len = self.deserialize_int()?;
        let mut v = Vec::with_capacity(len as usize);
        for _ in 0..len {
            v.push(self.deserialize_int()?);
        }
        Ok(v)
    }
    pub fn deserialize_string_array(&mut self) -> Result<Vec<String>> {
        let len = self.deserialize_short()?;
        let mut v = Vec::with_capacity(len as usize);
        for _ in 0..len {
            v.push(self.deserialize_string()?);
        }
        Ok(v)
    }
    pub fn deserialize_object_array(&mut self) -> Result<ObjectArray> {
        let len = self.deserialize_short()?;
        let mut v = Vec::with_capacity(len as usize);
        for _ in 0..len {
            let c = self.read_type_code()? as u8;
            v.push(self.deserialize(c)?);
        }
        Ok(v.into())
    }
    pub fn deserialize_hashtable(&mut self) -> Result<HashTable<Value, Value>> {
        let len = self.deserialize_short()?;
        let mut m = HashMap::with_capacity(len as usize);
        for _ in 0..len {
            let keytype = self.read_type_code()?;
            let key = self.deserialize(keytype as u8)?;
            let valtype = self.read_type_code()?;
            let val = self.deserialize(valtype as u8)?;
            m.insert(key, val);
        }
        Ok(HashTable(m.into()))
    }
    pub fn deserialize_dictionary(&mut self) -> Result<HashableHashmap<Value, Value>> {
        let (keytype, valtype) = self.deserialize_dictionary_type()?;
        let len = self.deserialize_short()?;
        let mut m = HashMap::with_capacity(len as usize);
        for _ in 0..len {
            let key = self.deserialize_maybe_typed(keytype)?;
            let val = self.deserialize_maybe_typed(valtype)?;
            m.insert(key, val);
        }
        Ok(m.into())
    }
    pub fn deserialize_parameter_table(&mut self) -> Result<ParameterTable> {
        let len = self.deserialize_short()?;
        let mut m = HashMap::with_capacity(len as usize);
        for _ in 0..len {
            let k = self.read_byte()?;
            let valtype = self.read_type_code()? as u8;
            let v = self.deserialize(valtype)?;
            m.insert(k, v);
        }
        Ok(ParameterTable(m.into()))
    }
    pub fn deserialize_event_data(&mut self) -> Result<EventData> {
        Ok(EventData {
            event_code: self.read_byte()?,
            params: self.deserialize_parameter_table()?,
        })
    }
    pub fn deserialize_operation_response(&mut self) -> Result<OperationResponse> {
        let opcode = self.read_byte()?; //WebServicesOpCode::from_repr(self.read_byte()?).unwrap();
        let return_code = self.deserialize_short()?;
        let dbg_msgtype = self.read_type_code()?;
        let debug_message = Box::new(self.deserialize(dbg_msgtype as u8)?);
        let parameters = self.deserialize_parameter_table()?;
        Ok(OperationResponse {
            opcode,
            return_code,
            debug_message,
            parameters,
        })
    }
    pub fn deserialize_operation_request(&mut self) -> Result<OperationRequest> {
        let opcode = self.read_byte()?;
        let parameters = self.deserialize_parameter_table()?;
        Ok(OperationRequest { opcode, parameters })
    }
}
