use inkwell::{builder::Builder, context::Context, types::{BasicTypeEnum, StructType}, values::{BasicValueEnum, PointerValue}};
use crate::{ast::{Variable}, index::{Index}};
use super::{CodeGen, VariableDeclarationInformation, statement_generator::StatementCodeGenerator, typesystem, variable_generator};

pub struct InstanceStructGenerator<'a, 'b> {
    context: &'a Context,
    global_index: &'b Index<'a>,
    pub local_index: Index<'a>,
}

impl<'a, 'b> InstanceStructGenerator<'a, 'b> {

    pub fn new(context: &'a Context, global_index: &'b Index<'a>) -> InstanceStructGenerator<'a, 'b> {
        InstanceStructGenerator{
            context,
            global_index,
            local_index: Index::new(),
        }       
    }

    pub fn generate_struct_type(
        &mut self,
        member_variables: &Vec<&Variable>,
        name: &str,
        builder: &Builder<'a>) -> Result<StructType<'a>, String> {

        let struct_type_info = self.global_index.find_type(name).unwrap();

        let struct_type = struct_type_info.get_type()
            .unwrap()
            .into_struct_type();

        let mut members = Vec::new();
        for member in member_variables {
            members.push(self.create_llvm_variable_declaration_elements(member, builder)?);
        }

        let member_types: Vec<BasicTypeEnum> = members.iter().map(|(_, t, _)| *t).collect();
        struct_type.set_body(member_types.as_slice(), false);
        
        let struct_fields_values = members.iter()
                .map(|(_,basic_type, initializer)| 
                    initializer.unwrap_or_else(|| typesystem::get_default_for(basic_type.clone())))
                .collect::<Vec<BasicValueEnum>>();

        let initial_value = struct_type.const_named_struct(struct_fields_values.as_slice());
        
        //associate in local index
        self.local_index.associate_type_initial_value(name, initial_value.into());
        
        //(struct_type, initial_value.as_basic_value_enum())
        Ok(struct_type)
    }







    fn create_llvm_variable_declaration_elements(&self,
            variable: &Variable,
            builder: &Builder<'a>,
        )->Result<VariableDeclarationInformation<'a>, String> {
            
            let type_name = variable.data_type.get_name().unwrap(); //TODO
            let type_index_entry = self.global_index.find_type(type_name)
                                    .ok_or(format!("Unknown datatype '{:}' at {:}", 
                                    &variable.data_type.get_name().unwrap_or("unknown"), 
                                    0/*self.new_lines.get_location_information(&variable.location)*/))?;


            let (variable_type, initializer) = match &variable.initializer {
                Some(statement) => {
                    let statement_gen = StatementCodeGenerator::new_typed(
                            self.context, 
                            self.global_index, 
                            None, 
                            type_index_entry.get_type().ok_or("unknown type")?); //TODO

                    statement_gen.generate_expression(statement, builder)
                        .map(|(data_type, value)| (data_type, Some(value)))?
                }
                None => 
                    (type_index_entry.get_type_information().unwrap().clone(), type_index_entry.get_initial_value())
            };

            Ok((variable.name.to_string(), variable_type.get_type(), initializer))
        }


    pub fn allocate_struct_instance(&self, builder: &Builder<'a>, callable_name: &str) -> Result<PointerValue<'a>, String> {
        let instance_name = CodeGen::get_struct_instance_name(callable_name);
        let function_type = self.global_index.get_type(callable_name)?
                                .get_type() //TODO Store as datatype in the index and fetch it?
                                .ok_or_else(|| format!("No type associated to {:}", callable_name))?;
        Ok(variable_generator::create_llvm_local_variable(builder, &instance_name, &function_type))
    }

}