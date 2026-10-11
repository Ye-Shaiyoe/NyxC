use crate::ast::Type;

pub fn is_assignable(target: &Type, source: &Type) -> bool {
    if target == source {
        return true;
    }

    match (target, source) {
        // Numeric promotions / conversions between integer types
        (Type::I32, Type::I8) | (Type::I32, Type::I16) | (Type::I32, Type::U8) | (Type::I32, Type::U16) => true,
        (Type::U32, Type::U8) | (Type::U32, Type::U16) => true,
        // Any pointer can convert to/from *u8 or void pointer (or integer 0/null in i32)
        (Type::Pointer(_), Type::Pointer(_)) => true,
        (Type::Pointer(_), Type::I32) | (Type::Pointer(_), Type::U32) => true,
        (Type::I32, Type::Pointer(_)) | (Type::U32, Type::Pointer(_)) => true,
        _ => false,
    }
}

/// An integer literal may initialize a narrower integer type when the value
/// fits, e.g. `w: i16 = 10`.
pub fn int_fits(ty: &Type, n: i64) -> bool {
    match ty {
        Type::I8 => (-128..=127).contains(&n),
        Type::I16 => (-32768..=32767).contains(&n),
        Type::I32 => (i32::MIN as i64..=i32::MAX as i64).contains(&n),
        Type::U8 => (0..=255).contains(&n),
        Type::U16 => (0..=65535).contains(&n),
        Type::U32 => (0..=u32::MAX as i64).contains(&n),
        _ => false,
    }
}
