//! API interface builder for TypeScript code generation

use std::collections::BTreeMap;

use crate::ast::{TsExpression, TsInterfaceDefinition, TsInterfaceSignature, TsProperty};
use crate::templating::data::{ApiClassData, MethodTemplateData};

/// Builder for API interface definitions
#[derive(Debug, Clone)]
pub struct ApiInterfaceBuilder;

impl ApiInterfaceBuilder {
    /// Build an interface definition from a class and method template data
    pub fn build_interface(
        &self,
        class: &ApiClassData,
        method_template_data: &BTreeMap<String, MethodTemplateData>,
    ) -> TsInterfaceDefinition {
        // Build interface signature (export interface FooInterface ...)
        let interface_name = format!("{}Interface", class.name);
        let interface_signature = TsInterfaceSignature::new(interface_name.clone(), interface_name)
            .with_generics(class.generics.clone());

        // Convert methods into function-typed properties for the interface
        let mut interface_properties: Vec<TsProperty> = class
            .methods
            .clone()
            .into_iter()
            .filter(|m| m.name != "constructor")
            .map(|m| {
                let func_type = TsExpression::Function {
                    parameters: m.parameters.clone(),
                    return_type: m.return_type.map(Box::new),
                };
                TsProperty {
                    ts_name: m.name.clone(),
                    original_name: m.name.clone(),
                    type_expr: func_type,
                    optional: false,
                    documentation: m.documentation.clone(),
                }
            })
            .collect();

        // Add convenience methods to the interface
        for (raw_method_name, template_data) in method_template_data {
            if let (Some(conv_name), Some(conv_return_type)) = (
                &template_data.convenience_method_name,
                &template_data.convenience_return_type,
            ) {
                // Find the corresponding Raw method to get its parameters
                if let Some(raw_method) = class.methods.iter().find(|m| m.name == *raw_method_name)
                {
                    // Wrap convenience return type in Promise
                    let type_str = conv_return_type.to_string_formatted();
                    let promise_return_type =
                        TsExpression::Reference(format!("Promise<{}>", type_str));
                    let func_type = TsExpression::Function {
                        parameters: raw_method.parameters.clone(),
                        return_type: Some(Box::new(promise_return_type)),
                    };
                    interface_properties.push(TsProperty {
                        ts_name: conv_name.clone(),
                        original_name: conv_name.clone(),
                        type_expr: func_type,
                        optional: false,
                        documentation: raw_method.documentation.clone(),
                    });
                }
            }
        }

        TsInterfaceDefinition::new(interface_signature).with_properties(interface_properties)
    }
}
