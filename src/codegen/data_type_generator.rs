use inkwell::{
    context::Context,
    module::Module,
    types::{ArrayType, BasicType, BasicTypeEnum},
    values::{BasicValue},
};
use crate::{
    ast::{DataType, DataTypeDeclaration, Statement, Variable},
    index::{DataTypeInformation, Dimension, Index},
};

use std::io::{self, Write};

use super::{
    instance_struct_generator::InstanceStructGenerator, variable_generator,
};

pub fn generate_data_type_stubs<'a>(context: &'a Context, index: &mut Index<'a>, data_types: &Vec<DataType>) {
    for data_type in data_types {
        match data_type {
            DataType::StructType { name, variables: _ } => {
                index.associate_type(
                    name.as_ref().unwrap().as_str(),
                    DataTypeInformation::Struct {
                        name: name.clone().unwrap(),
                        generated_type: context
                            .opaque_struct_type(name.as_ref().unwrap())
                            .into(),
                    },
                );
            }
            DataType::EnumType { name, elements: _ } => index.associate_type(
                name.as_ref().unwrap().as_str(),
                DataTypeInformation::Integer {
                    signed: true,
                    size: 32,
                    generated_type: context.i32_type().as_basic_type_enum(),
                },
            ),
            DataType::SubRangeType {
                name,
                referenced_type: type_ref_name,
                initializer: _,
            } => {
                let alias_name = name.as_ref().unwrap();
                index.associate_type(
                    alias_name,
                    DataTypeInformation::Alias {
                        name: alias_name.clone(),
                        referenced_type: type_ref_name.clone(),
                    },
                );
                //TODO error handling
                //let ref_type = self.index.find_type(name.as_str()).unwrap();
                //self.index.associate_type(name, ref_type.get_type_information().unwrap().clone());
                /*if let Some(initializer_statement) = initializer {
                    let (Some(initial_value), _) = self.generate_statement(initializer_statement)?;
                    //TODO add cast
                    self.index.associate_type_initial_value(name, initial_value)
                }*/
            }
            DataType::ArrayType {
                name,
                bounds,
                referenced_type,
            } => {
                let dimensions = get_array_dimensions(bounds);
                let target_type = get_type(index,referenced_type).unwrap();
                let internal_type = index
                    .find_type_information(referenced_type.get_name().unwrap())
                    .unwrap();
                index.associate_type(
                    name.as_ref().unwrap().as_str(),
                    DataTypeInformation::Array {
                        name: name.as_ref().unwrap().clone(),
                        internal_type_information: Box::new(internal_type),
                        generated_type: create_nested_array_type(
                            target_type,
                            dimensions.clone(),
                        )
                        .as_basic_type_enum(),
                        dimensions,
                    },
                )
            }
        };
    }
}

pub fn generate_data_type<'a>(
    module: &Module<'a>,
    context: &'a Context,
    index: &mut Index<'a>,
    data_types: &Vec<DataType>,
) -> Result<(), String> {
    for data_type in data_types {
        match data_type {
            DataType::StructType { name, variables } => {
                let name = name.as_ref().unwrap();
                let mut struct_generator = InstanceStructGenerator::new(context, index);
                let members: Vec<&Variable> = variables.iter().collect();
                struct_generator.generate_struct_type(&members, name, &context.create_builder())?;
            }
            DataType::EnumType { name: _, elements } => {
                for (i, element) in elements.iter().enumerate() {
                    let int_type = context.i32_type();
                    let element_variable = variable_generator::create_llvm_global_variable(
                        module,
                        element,
                        int_type.as_basic_type_enum(),
                        Some(int_type.const_int(i as u64, false).as_basic_value_enum()),
                    );

                    //associate the enum element's global variable
                    index.associate_global_variable(element, element_variable.as_pointer_value());
                }
            }
            DataType::SubRangeType {
                name,
                referenced_type,
                initializer,
            } => {
                let alias_name = name.as_ref().map(|it| it.as_str()).unwrap();
                index.associate_type_alias(alias_name, referenced_type.as_str());
                if let Some(initializer_statement) = initializer {

                    //TODO
                    //let (_, initial_value) = self.generate_statement(initializer_statement)?;
                    //index.associate_type_initial_value(alias_name, initial_value.ok_or_else(|| "could not generate initial value for TODO")?);
                }
            }
            DataType::ArrayType { .. } => {}
        }
    }
    Ok(())
}

fn get_array_dimensions(bounds: &Statement) -> Vec<Dimension> {
    let mut result = vec![];
    for statement in bounds.get_as_list() {
        result.push(get_single_array_dimension(statement).expect("Could not parse array bounds"));
    }
    result
}

fn get_single_array_dimension(bounds: &Statement) -> Result<Dimension, String> {
    if let Statement::RangeStatement { start, end } = bounds {
        let start_offset = evaluate_constant_int(start).unwrap_or(0);
        let end_offset = evaluate_constant_int(end).unwrap_or(0);
        Ok(Dimension {
            start_offset,
            end_offset,
        })
    } else {
        Err(format!("Unexpected Statement {:?}, expected range", bounds))
    }
}

fn extract_value(s: &Statement) -> Option<String> {
    match s {
        Statement::UnaryExpression {
            operator, value, ..
        } => extract_value(value).map(|result| format!("{}{}", operator, result)),
        Statement::LiteralInteger { value, .. } => Some(value.to_string()),
        //TODO constatnts
        _ => None,
    }
}

fn evaluate_constant_int(s: &Statement) -> Option<i32> {
    let value = extract_value(s);
    value.map(|v| v.parse().unwrap_or(0))
}

fn get_type<'a>(index: &Index<'a>, data_type: &DataTypeDeclaration) -> Option<BasicTypeEnum<'a>> {
    data_type
        .get_name()
        .and_then(|name| index.find_type(name).map(|it| it.get_type()).flatten())
}

fn create_nested_array_type(end_type: BasicTypeEnum, dimensions: Vec<Dimension>) -> ArrayType {
    let mut result: Option<ArrayType> = None;
    let mut current_type = end_type;
    for dimension in dimensions.iter().rev() {
        result = Some(match current_type {
            BasicTypeEnum::IntType(..) => current_type
                .into_int_type()
                .array_type(dimension.get_length()),
            BasicTypeEnum::FloatType(..) => current_type
                .into_float_type()
                .array_type(dimension.get_length()),
            BasicTypeEnum::StructType(..) => current_type
                .into_struct_type()
                .array_type(dimension.get_length()),
            BasicTypeEnum::ArrayType(..) => current_type
                .into_array_type()
                .array_type(dimension.get_length()),
            BasicTypeEnum::PointerType(..) => current_type
                .into_pointer_type()
                .array_type(dimension.get_length()),
            BasicTypeEnum::VectorType(..) => current_type
                .into_vector_type()
                .array_type(dimension.get_length()),
        });
        current_type = result.unwrap().into();
    }
    result.unwrap()
}

/*///
/// returns the generated type and it's optional initializer
///
fn generate_struct_type<'a>(
    index: &Index<'a>,
    members: &Vec<(String, BasicTypeEnum<'a>, Option<BasicValueEnum<'a>>)>,
    name: &str,
) -> (StructType<'a>, BasicValueEnum<'a>) {
    let struct_type_info = index
        .find_type(name)
        .unwrap();

    let struct_type = struct_type_info.get_type()
        .unwrap()
        .into_struct_type();
    let member_types: Vec<BasicTypeEnum> = members.iter().map(|(_, t, _)| *t).collect();
    struct_type.set_body(member_types.as_slice(), false);

    let struct_fields_values = members.iter()
            .map(|(_,basic_type, initializer)|

                initializer
                    .unwrap_or_else(|| typesystem::get_default_for(basic_type.clone())

                ))
            .collect::<Vec<BasicValueEnum>>();

    let initial_value = struct_type.const_named_struct(struct_fields_values.as_slice());
    index.associate_type_initial_value(name, initial_value.into());

    (struct_type, initial_value.as_basic_value_enum())
}*/
