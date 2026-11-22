//! Convert slot-based analysis frames into classfile verification types.
use super::{ConstantPoolBuilder, item_offset, model_error};
use crate::analysis::{FrameComputationResult, VType};
use crate::error::Result;
use crate::raw::{AttributeInfo, StackMapFrameInfo, VerificationTypeInfo};

pub(super) fn lower_stack_map_table(
    frames: &FrameComputationResult,
    initial_locals: &[VType],
    cp: &mut ConstantPoolBuilder,
    item_offsets: &[Option<u32>],
) -> Result<AttributeInfo> {
    let mut prev_locals = collapse_slots(initial_locals, item_offsets, true)?;
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
            let next_min_offset = previous_offset
                .checked_add(1)
                .ok_or_else(|| model_error("stack-map previous offset overflows u32"))?;
            offset
                .checked_sub(next_min_offset)
                .ok_or_else(|| model_error("stack-map frame offsets are not monotonic"))?
        };
        let delta_u16 = u16::try_from(offset_delta)
            .map_err(|_| model_error("stack-map offset delta exceeds 65535"))?;
        previous_offset = offset;

        let curr_locals = collapse_slots(&frame.locals, item_offsets, true)?;
        let curr_stack = collapse_slots(&frame.stack, item_offsets, false)?;

        let frame_info =
            encode_delta_frame(delta_u16, &prev_locals, &curr_locals, &curr_stack, cp)?;
        entries.push(frame_info);
        prev_locals = curr_locals;
    }

    Ok(AttributeInfo::StackMapTable(
        crate::raw::StackMapTableAttribute {
            attribute_name_index: cp.add_utf8("StackMapTable")?,
            attribute_length: stack_map_table_length(&entries),
            entries,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SlotVerificationType {
    Top,
    Integer,
    Float,
    Double,
    Long,
    Null,
    UninitializedThis,
    Object(String),
    Uninitialized(u16),
}

impl SlotVerificationType {
    fn from_vtype(value: &VType, item_offsets: &[Option<u32>]) -> Result<Self> {
        match value {
            VType::Top => Ok(Self::Top),
            VType::Integer => Ok(Self::Integer),
            VType::Float => Ok(Self::Float),
            VType::Double => Ok(Self::Double),
            VType::Long => Ok(Self::Long),
            VType::Null => Ok(Self::Null),
            VType::ReturnAddress(_) => Err(model_error(
                "StackMapTable cannot encode returnAddress verification types",
            )),
            VType::UninitializedThis => Ok(Self::UninitializedThis),
            VType::Object(class_name) => Ok(Self::Object(class_name.clone())),
            VType::Uninitialized { code_index, .. } => {
                let offset = item_offset(
                    item_offsets,
                    *code_index,
                    "missing offset for uninitialized new instruction",
                )?;
                let offset_u16 = u16::try_from(offset).map_err(|_| {
                    model_error("uninitialized new instruction offset exceeds 65535")
                })?;
                Ok(Self::Uninitialized(offset_u16))
            }
        }
    }

    fn to_raw(&self, cp: &mut ConstantPoolBuilder) -> Result<VerificationTypeInfo> {
        Ok(match self {
            Self::Top => VerificationTypeInfo::Top,
            Self::Integer => VerificationTypeInfo::Integer,
            Self::Float => VerificationTypeInfo::Float,
            Self::Double => VerificationTypeInfo::Double,
            Self::Long => VerificationTypeInfo::Long,
            Self::Null => VerificationTypeInfo::Null,
            Self::UninitializedThis => VerificationTypeInfo::UninitializedThis,
            Self::Object(class_name) => VerificationTypeInfo::Object {
                cpool_index: cp.add_class(class_name)?,
            },
            Self::Uninitialized(offset) => VerificationTypeInfo::Uninitialized { offset: *offset },
        })
    }
}

fn collapse_slots(
    slots: &[VType],
    item_offsets: &[Option<u32>],
    trim_trailing_top: bool,
) -> Result<Vec<SlotVerificationType>> {
    let mut types = Vec::with_capacity(slots.len());
    let mut values = slots.iter();
    while let Some(value) = values.next() {
        types.push(SlotVerificationType::from_vtype(value, item_offsets)?);
        // The analysis state counts JVM slots. StackMapTable encodes long/double
        // as one verification_type_info, which implicitly occupies two slots.
        if matches!(value, VType::Long | VType::Double) && values.next() != Some(&VType::Top) {
            return Err(model_error(
                "category-2 frame value is missing its second slot",
            ));
        }
    }
    if trim_trailing_top {
        while matches!(types.last(), Some(SlotVerificationType::Top)) {
            types.pop();
        }
    }
    Ok(types)
}

fn encode_delta_frame(
    offset_delta: u16,
    prev_locals: &[SlotVerificationType],
    curr_locals: &[SlotVerificationType],
    curr_stack: &[SlotVerificationType],
    cp: &mut ConstantPoolBuilder,
) -> Result<StackMapFrameInfo> {
    if curr_stack.is_empty() && curr_locals == prev_locals {
        return Ok(if offset_delta <= 63 {
            StackMapFrameInfo::Same {
                frame_type: offset_delta as u8,
            }
        } else {
            StackMapFrameInfo::SameExtended {
                frame_type: 251,
                offset_delta,
            }
        });
    }

    if curr_stack.len() == 1 && curr_locals == prev_locals {
        let stack = curr_stack[0].to_raw(cp)?;
        return Ok(if offset_delta <= 63 {
            StackMapFrameInfo::SameLocals1StackItem {
                frame_type: (offset_delta + 64) as u8,
                stack,
            }
        } else {
            StackMapFrameInfo::SameLocals1StackItemExtended {
                frame_type: 247,
                offset_delta,
                stack,
            }
        });
    }

    if curr_stack.is_empty() {
        if curr_locals.len() < prev_locals.len()
            && prev_locals.len() - curr_locals.len() <= 3
            && prev_locals[..curr_locals.len()] == curr_locals[..]
        {
            let chop_count = prev_locals.len() - curr_locals.len();
            return Ok(StackMapFrameInfo::Chop {
                frame_type: (251 - chop_count) as u8,
                offset_delta,
            });
        }
        if curr_locals.len() > prev_locals.len()
            && curr_locals.len() - prev_locals.len() <= 3
            && curr_locals[..prev_locals.len()] == prev_locals[..]
        {
            let append_count = curr_locals.len() - prev_locals.len();
            let locals = curr_locals[prev_locals.len()..]
                .iter()
                .map(|slot| slot.to_raw(cp))
                .collect::<Result<Vec<_>>>()?;
            return Ok(StackMapFrameInfo::Append {
                frame_type: (251 + append_count) as u8,
                offset_delta,
                locals,
            });
        }
    }

    let _ = u16::try_from(curr_locals.len())
        .map_err(|_| model_error("stack-map locals count exceeds 65535"))?;
    let _ = u16::try_from(curr_stack.len())
        .map_err(|_| model_error("stack-map stack depth exceeds 65535"))?;
    let locals = curr_locals
        .iter()
        .map(|slot| slot.to_raw(cp))
        .collect::<Result<Vec<_>>>()?;
    let stack = curr_stack
        .iter()
        .map(|slot| slot.to_raw(cp))
        .collect::<Result<Vec<_>>>()?;
    Ok(StackMapFrameInfo::Full {
        frame_type: 255,
        offset_delta,
        locals,
        stack,
    })
}

fn verification_type_size(vtype: &VerificationTypeInfo) -> u32 {
    match vtype {
        VerificationTypeInfo::Object { .. } | VerificationTypeInfo::Uninitialized { .. } => 3,
        _ => 1,
    }
}

fn stack_map_table_length(entries: &[StackMapFrameInfo]) -> u32 {
    let mut len = 2_u32;
    for entry in entries {
        len = len.saturating_add(match entry {
            StackMapFrameInfo::Same { .. } => 1,
            StackMapFrameInfo::SameLocals1StackItem { stack, .. } => {
                1 + verification_type_size(stack)
            }
            StackMapFrameInfo::SameLocals1StackItemExtended { stack, .. } => {
                3 + verification_type_size(stack)
            }
            StackMapFrameInfo::Chop { .. } | StackMapFrameInfo::SameExtended { .. } => 3,
            StackMapFrameInfo::Append { locals, .. } => {
                3 + locals.iter().map(verification_type_size).sum::<u32>()
            }
            StackMapFrameInfo::Full { locals, stack, .. } => {
                7 + locals.iter().map(verification_type_size).sum::<u32>()
                    + stack.iter().map(verification_type_size).sum::<u32>()
            }
        });
    }
    len
}
