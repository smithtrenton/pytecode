//! Convert slot-based analysis frames into classfile verification types.
use super::{ConstantPoolBuilder, item_offset, model_error};
use crate::analysis::{FrameComputationResult, VType};
use crate::error::Result;
use crate::raw::AttributeInfo;

pub(super) fn lower_stack_map_table(
    frames: &FrameComputationResult,
    cp: &mut ConstantPoolBuilder,
    item_offsets: &[Option<u32>],
) -> Result<AttributeInfo> {
    let mut entries = Vec::with_capacity(frames.frames.len());
    let mut previous_offset = 0_u32;
    for (frame_index, frame) in frames.frames.iter().enumerate() {
        let offset = item_offset(
            item_offsets,
            frame.code_index,
            "stack-map frame instruction offset missing",
        )?;
        let offset_delta = if frame_index == 0 {
            offset
        } else {
            offset
                .checked_sub(previous_offset + 1)
                .ok_or_else(|| model_error("stack-map frame offsets are not monotonic"))?
        };
        previous_offset = offset;
        let locals = raw_verification_types(&frame.locals, cp, item_offsets)?;
        let stack = raw_verification_types(&frame.stack, cp, item_offsets)?;
        entries.push(crate::raw::StackMapFrameInfo::Full {
            frame_type: 255,
            offset_delta: u16::try_from(offset_delta)
                .map_err(|_| model_error("stack-map offset delta exceeds 65535"))?,
            locals,
            stack,
        });
    }
    Ok(AttributeInfo::StackMapTable(
        crate::raw::StackMapTableAttribute {
            attribute_name_index: cp.add_utf8("StackMapTable")?,
            attribute_length: 2,
            entries,
        },
    ))
}

fn raw_verification_types(
    slots: &[VType],
    cp: &mut ConstantPoolBuilder,
    item_offsets: &[Option<u32>],
) -> Result<Vec<crate::raw::VerificationTypeInfo>> {
    let mut types = Vec::with_capacity(slots.len());
    let mut values = slots.iter();
    while let Some(value) = values.next() {
        types.push(raw_verification_type(value, cp, item_offsets)?);
        // The analysis state counts JVM slots. StackMapTable encodes long/double
        // as one verification_type_info, which implicitly occupies two slots.
        if matches!(value, VType::Long | VType::Double) && values.next() != Some(&VType::Top) {
            return Err(model_error(
                "category-2 frame value is missing its second slot",
            ));
        }
    }
    Ok(types)
}

fn raw_verification_type(
    value: &VType,
    cp: &mut ConstantPoolBuilder,
    item_offsets: &[Option<u32>],
) -> Result<crate::raw::VerificationTypeInfo> {
    match value {
        VType::Top => Ok(crate::raw::VerificationTypeInfo::Top),
        VType::Integer => Ok(crate::raw::VerificationTypeInfo::Integer),
        VType::Float => Ok(crate::raw::VerificationTypeInfo::Float),
        VType::Double => Ok(crate::raw::VerificationTypeInfo::Double),
        VType::Long => Ok(crate::raw::VerificationTypeInfo::Long),
        VType::Null => Ok(crate::raw::VerificationTypeInfo::Null),
        VType::ReturnAddress(_) => Err(model_error(
            "StackMapTable cannot encode returnAddress verification types",
        )),
        VType::UninitializedThis => Ok(crate::raw::VerificationTypeInfo::UninitializedThis),
        VType::Object(class_name) => Ok(crate::raw::VerificationTypeInfo::Object {
            cpool_index: cp.add_class(class_name)?,
        }),
        VType::Uninitialized { code_index, .. } => {
            let offset = item_offset(
                item_offsets,
                *code_index,
                "missing offset for uninitialized new instruction",
            )?;
            Ok(crate::raw::VerificationTypeInfo::Uninitialized {
                offset: offset as u16,
            })
        }
    }
}
