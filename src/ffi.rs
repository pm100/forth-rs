use std::ffi::CString;
use dyncall::{ArgType, FuncDef};
use crate::errors::Error;
use crate::forth::Forth;
use crate::numbers::Int;

/// Dispatch a foreign function call.
///
/// Arguments are consumed from the data stack in declaration order
/// (the first declared argument is deepest on the stack).
/// The return value (if any) is pushed onto the data stack.
pub fn dispatch(func_def: &FuncDef, forth: &mut Forth) -> Result<(), Error> {
    let arg_count = func_def.get_arg_count();

    // Pop args from stack — they were pushed first-arg-first, so last arg is on top.
    let mut raw: Vec<Int> = (0..arg_count)
        .map(|_| forth.stack_pop())
        .collect::<Result<_, _>>()?;
    raw.reverse(); // raw[0] = first arg

    let mut inv = func_def.prep();
    // Keep CStrings alive for the duration of the call.
    let mut owned_cstrings: Vec<CString> = Vec::new();

    for (i, &val) in raw.iter().enumerate() {
        match func_def.get_arg_type(i) {
            ArgType::Char => inv.push_arg(&(val as u8)),
            ArgType::I16 => inv.push_arg(&(val as i16)),
            ArgType::U16 => inv.push_arg(&(val as u16)),
            ArgType::I32 => inv.push_arg(&(val as i32)),
            ArgType::U32 => inv.push_arg(&(val as u32)),
            ArgType::I64 => inv.push_arg(&(val as i64)),
            ArgType::U64 => inv.push_arg(&(val as u64)),
            ArgType::F32 => inv.push_arg(&f32::from_bits(val as u32)),
            ArgType::F64 => inv.push_arg(&f64::from_bits(val as u64)),
            ArgType::CString => {
                // val is an index into forth.strings
                let idx = val as usize;
                if idx >= forth.strings.len() {
                    return Err(Error::FfiError(format!("invalid string index {}", idx)));
                }
                owned_cstrings.push(forth.strings[idx].clone());
                let cstr = owned_cstrings.last().unwrap();
                inv.push_arg(cstr.as_c_str());
            }
            ArgType::OpaquePointer => {
                let ptr = val as usize as *mut std::ffi::c_void;
                inv.push_arg(&dyncall::ArgVal::Pointer(ptr));
            }
            other => {
                return Err(Error::FfiError(format!(
                    "unsupported argument type {:?} at position {}",
                    other, i
                )));
            }
        }
    }

    let result = inv.call();

    match func_def.get_return_type() {
        ArgType::Void => {}
        ArgType::Char => forth.stack_push(*result.as_char().unwrap() as Int),
        ArgType::I16 => forth.stack_push(*result.as_i16().unwrap() as Int),
        ArgType::U16 => forth.stack_push(*result.as_u16().unwrap() as Int),
        ArgType::I32 => forth.stack_push(*result.as_i32().unwrap() as Int),
        ArgType::U32 => forth.stack_push(*result.as_u32().unwrap() as Int),
        ArgType::I64 => forth.stack_push(*result.as_i64().unwrap() as Int),
        ArgType::U64 => forth.stack_push(*result.as_u64().unwrap() as Int),
        ArgType::F32 => forth.stack_push(result.as_f32().unwrap().to_bits() as Int),
        ArgType::F64 => forth.stack_push(f64::to_bits(*result.as_f64().unwrap()) as Int),
        ArgType::OpaquePointer => {
            forth.stack_push(*result.as_pointer().unwrap() as usize as Int);
        }
        other => {
            return Err(Error::FfiError(format!(
                "unsupported return type {:?}",
                other
            )));
        }
    }

    Ok(())
}
