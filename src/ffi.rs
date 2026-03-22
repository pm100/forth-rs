use std::ffi::CString;
use dyncall::{ArgType, ArgVal, FuncDef, ScriptVal, StructValue};
use crate::errors::Error;
use crate::forth::Forth;
use crate::numbers::Int;

fn is_struct_arg(arg_type: &ArgType) -> bool {
    arg_type.struct_type().is_some()
}

/// Number of data-stack slots an argument occupies.
/// Struct args occupy one slot per field; all others occupy 1.
fn slot_count(arg_type: &ArgType) -> usize {
    arg_type.struct_type()
        .map(|st| st.field_count())
        .unwrap_or(1)
}

/// Dispatch a foreign function call.
///
/// Scalar arguments are consumed from the data stack in declaration order
/// (the first declared argument is deepest on the stack).
///
/// Struct / pointer-to-struct arguments occupy one stack slot per field in
/// declaration order; the first field is deepest on the stack.
///
/// The return value (if any) is pushed onto the data stack.  Struct returns
/// push one value per field in declaration order (first field deepest).
pub fn dispatch(func_def: &FuncDef, forth: &mut Forth) -> Result<(), Error> {
    let arg_count = func_def.get_arg_count();

    // Compute slot counts so we know exactly how many values to pop.
    let slot_counts: Vec<usize> = (0..arg_count)
        .map(|i| slot_count(func_def.get_arg_type(i)))
        .collect();
    let total_slots: usize = slot_counts.iter().sum();

    // Pop all slots from stack (last slot of last arg is on top).
    let mut raw: Vec<Int> = (0..total_slots)
        .map(|_| forth.stack_pop())
        .collect::<Result<_, _>>()?;
    raw.reverse(); // raw[0] = first slot of first arg

    let mut inv = func_def.prep();
    // Keep CStrings and StructValues alive for the duration of the call.
    let mut owned_cstrings: Vec<CString> = Vec::new();

    // Pre-build StructValues for all struct args before pushing, so their
    // addresses are stable while the invocation holds pointers to them.
    let mut struct_values: Vec<Option<StructValue>> = (0..arg_count).map(|_| None).collect();
    let mut offset = 0usize;
    for i in 0..arg_count {
        let n = slot_counts[i];
        if is_struct_arg(func_def.get_arg_type(i)) {
            let script_vals: Vec<dyncall::ScriptVal> = raw[offset..offset + n]
                .iter()
                .map(|&v| dyncall::ScriptVal::Number(v as f64))
                .collect();
            let sv = StructValue::from_script_vals(func_def.get_arg_type(i), &script_vals)
                .map_err(|e| Error::FfiError(e.to_string()))?;
            struct_values[i] = Some(sv);
        }
        offset += n;
    }

    // Push args in order.
    let mut offset = 0usize;
    for i in 0..arg_count {
        let n = slot_counts[i];
        let arg_type = func_def.get_arg_type(i);

        if is_struct_arg(arg_type) {
            let sv = struct_values[i].as_mut().unwrap();
            let is_ptr = matches!(arg_type, ArgType::Pointer(_));
            if is_ptr {
                inv.push_mut_arg(sv).map_err(|e| Error::FfiError(e.to_string()))?;
            } else {
                inv.push_arg(sv).map_err(|e| Error::FfiError(e.to_string()))?;
            }
            offset += n;
            continue;
        }

        let val = raw[offset];
        match arg_type {
            ArgType::Char => inv.push_arg(&(val as u8)).map_err(|e| Error::FfiError(e.to_string()))?,
            ArgType::I16 => inv.push_arg(&(val as i16)).map_err(|e| Error::FfiError(e.to_string()))?,
            ArgType::U16 => inv.push_arg(&(val as u16)).map_err(|e| Error::FfiError(e.to_string()))?,
            ArgType::I32 => inv.push_arg(&(val as i32)).map_err(|e| Error::FfiError(e.to_string()))?,
            ArgType::U32 => inv.push_arg(&(val as u32)).map_err(|e| Error::FfiError(e.to_string()))?,
            ArgType::I64 => inv.push_arg(&val).map_err(|e| Error::FfiError(e.to_string()))?,
            ArgType::U64 => inv.push_arg(&(val as u64)).map_err(|e| Error::FfiError(e.to_string()))?,
            ArgType::F32 => inv.push_arg(&f32::from_bits(val as u32)).map_err(|e| Error::FfiError(e.to_string()))?,
            ArgType::F64 => inv.push_arg(&f64::from_bits(val as u64)).map_err(|e| Error::FfiError(e.to_string()))?,
            ArgType::CString => {
                // val is an index into forth.strings
                let idx = val as usize;
                if idx >= forth.strings.len() {
                    return Err(Error::FfiError(format!("invalid string index {}", idx)));
                }
                owned_cstrings.push(forth.strings[idx].clone());
                let cstr = owned_cstrings.last().unwrap();
                inv.push_arg(cstr.as_c_str()).map_err(|e| Error::FfiError(e.to_string()))?;
            }
            ArgType::OpaquePointer => {
                let ptr = val as usize as *mut std::ffi::c_void;
                inv.push_arg(&ArgVal::Pointer(ptr)).map_err(|e| Error::FfiError(e.to_string()))?;
            }
            other => {
                return Err(Error::FfiError(format!(
                    "unsupported argument type {:?} at position {}",
                    other, i
                )));
            }
        }
        offset += 1;
    }

    let result = inv.call().map_err(|e| Error::FfiError(e.to_string()))?;

    // Struct returns: push one value per field (first field deepest).
    if let ArgVal::StructValue(sv) = &result {
        for fi in 0..sv.field_count() {
            match sv.script_read(fi).map_err(|e| Error::FfiError(e.to_string()))? {
                ScriptVal::Number(n) => forth.stack_push(n as Int),
                ScriptVal::Str(s) => {
                    let idx = forth.strings.len();
                    forth.strings.push(CString::new(s).unwrap_or_default());
                    forth.stack_push(idx as Int);
                }
            }
        }
        return Ok(());
    }

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
