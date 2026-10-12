use std::ffi::CString;
use dyncall::{ArgType, FuncDef, ScriptVal, StructValue};
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

    // Push args in order. Track which struct args are pointer (`*{...}`) args,
    // since those get mutated in place and their fields are written back to
    // the data stack after the call.
    let mut writeback: Vec<bool> = (0..arg_count).map(|_| false).collect();
    let mut offset = 0usize;
    // Hand out each struct slot exactly once so the borrows pushed into the
    // invocation are provably disjoint.
    let mut sv_iter = struct_values.iter_mut();
    for i in 0..arg_count {
        let n = slot_counts[i];
        let arg_type = func_def.get_arg_type(i);
        let sv_slot = sv_iter.next().unwrap();

        if is_struct_arg(arg_type) {
            let sv = sv_slot.as_mut().unwrap();
            let is_ptr = matches!(arg_type, ArgType::Pointer(_));
            writeback[i] = is_ptr;
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
            ArgType::Char
            | ArgType::I8
            | ArgType::I16
            | ArgType::U16
            | ArgType::I32
            | ArgType::U32
            | ArgType::I64
            | ArgType::U64 => inv.push_script_val(ScriptVal::Integer(val)).map_err(|e| Error::FfiError(e.to_string()))?,
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
                inv.push_script_val(ScriptVal::Integer(val)).map_err(|e| Error::FfiError(e.to_string()))?;
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

    let script_result = inv.call_scripted().map_err(|e| Error::FfiError(e.to_string()))?;

    // Write mutated `*{...}` struct fields back to the data stack, in the same
    // slots (and field order) they were originally popped from, so the caller
    // can read them the way it would any other multi-value return.
    for i in 0..arg_count {
        if !writeback[i] {
            continue;
        }
        let sv = struct_values[i].as_ref().unwrap();
        for fi in 0..sv.field_count() {
            let val = match sv.script_read(fi) {
                Ok(ScriptVal::Integer(n)) => n as Int,
                Ok(ScriptVal::Number(f)) => f as Int,
                Ok(ScriptVal::Pointer(p)) => p as usize as Int,
                _ => 0,
            };
            forth.stack_push(val);
        }
    }

    // `*{...}` return: push one value per field (first field deepest), like the
    // arguments. cstr fields become string-table indices.
    if let Some(sv) = &script_result.return_struct {
        for fi in 0..sv.field_count() {
            let val = match sv.script_read(fi) {
                Ok(ScriptVal::Integer(n)) => n as Int,
                Ok(ScriptVal::Number(f)) => match sv.struct_type().field_type(fi) {
                    Some(ArgType::F32) => (f as f32).to_bits() as Int,
                    Some(ArgType::F64) => f64::to_bits(f) as Int,
                    _ => f as Int,
                },
                Ok(ScriptVal::Pointer(p)) => p as usize as Int,
                Ok(ScriptVal::Str(text)) => {
                    let idx = forth.strings.len();
                    forth.strings.push(CString::new(text).unwrap_or_default());
                    idx as Int
                }
                _ => 0,
            };
            forth.stack_push(val);
        }
    }

    match script_result.return_val {
        // A NULL pointer / NULL `cstr` return pushes 0; only `void` pushes
        // nothing (and a `*{...}` return has already pushed its fields).
        ScriptVal::Nil => {
            if script_result.return_struct.is_none()
                && !matches!(func_def.get_return_type(), ArgType::Void)
            {
                forth.stack_push(0);
            }
        }
        ScriptVal::Integer(n) => forth.stack_push(n as Int),
        ScriptVal::Number(f) => {
            // Forth stores floats as bit patterns on the stack.
            match func_def.get_return_type() {
                ArgType::F32 => forth.stack_push((f as f32).to_bits() as Int),
                _ => forth.stack_push(f64::to_bits(f) as Int),
            }
        }
        ScriptVal::Pointer(p) => forth.stack_push(p as usize as Int),
        ScriptVal::Str(s) => {
            let idx = forth.strings.len();
            forth.strings.push(CString::new(s).unwrap_or_default());
            forth.stack_push(idx as Int);
        }
    }

    Ok(())
}
