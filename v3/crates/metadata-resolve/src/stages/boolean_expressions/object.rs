use super::error::BooleanExpressionError;
use super::graphql;
use super::helpers;
use super::BooleanExpressionIssue;
pub use super::{
    BooleanExpressionComparableRelationship, ComparableFieldKind, ComparisonExpressionInfo,
    ObjectComparisonExpressionInfo, ObjectComparisonKind, OperatorMapping,
    ResolvedObjectBooleanExpressionType, ResolvedObjectBooleanExpressionTypeFields,
};
use crate::stages::{
    graphql_config, object_types, relationships, scalar_boolean_expressions, type_permissions,
};
use crate::types::subgraph::mk_qualified_type_name;
use crate::{Qualified, QualifiedBaseType};
use lang_graphql::ast::common::{self as ast};
use open_dds::identifier::SubgraphName;
use open_dds::{
    boolean_expression::{
        BooleanExpressionComparableField, BooleanExpressionLogicalOperators,
        BooleanExpressionObjectAggregateOperand, BooleanExpressionObjectOperand,
        BooleanExpressionOperand, BooleanExpressionScalarAggregateOperand,
        BooleanExpressionScalarOperand, BooleanExpressionTypeGraphQlConfiguration,
        DataConnectorOperatorMapping,
    },
    data_connector::DataConnectorName,
    types::{CustomTypeName, FieldName, TypeName},
};

use std::collections::{BTreeMap, BTreeSet};

pub(crate) type RawBooleanExpressionTypes<'a> = BTreeMap<
    Qualified<CustomTypeName>,
    (
        &'a open_dds::identifier::SubgraphName,
        &'a open_dds::boolean_expression::BooleanExpressionTypeV1,
    ),
>;

/// Resolves a given object boolean expression type
pub(crate) fn resolve_object_boolean_expression_type(
    boolean_expression_type_name: &Qualified<CustomTypeName>,
    object_boolean_expression_operand: &BooleanExpressionObjectOperand,
    logical_operators: &BooleanExpressionLogicalOperators,
    subgraph: &SubgraphName,
    graphql: &Option<BooleanExpressionTypeGraphQlConfiguration>,
    object_types: &BTreeMap<Qualified<CustomTypeName>, type_permissions::ObjectTypeWithPermissions>,
    scalar_boolean_expression_types: &BTreeMap<
        Qualified<CustomTypeName>,
        scalar_boolean_expressions::ResolvedScalarBooleanExpressionType,
    >,
    raw_boolean_expression_types: &RawBooleanExpressionTypes,
    relationships: &relationships::Relationships,
    graphql_config: &graphql_config::GraphqlConfig,
    graphql_types: &mut BTreeSet<ast::TypeName>,
    flags: &open_dds::flags::Flags,
) -> Result<
    (
        ResolvedObjectBooleanExpressionType,
        Vec<BooleanExpressionIssue>,
    ),
    BooleanExpressionError,
> {
    let qualified_object_type_name = Qualified::new(
        subgraph.clone(),
        object_boolean_expression_operand.r#type.clone(),
    );

    // get the underlying object type
    let object_type_representation =
        object_types
            .get(&qualified_object_type_name)
            .ok_or_else(
                || BooleanExpressionError::UnknownTypeInObjectBooleanExpressionType {
                    type_name: qualified_object_type_name.clone(),
                },
            )?;

    // resolve any comparable fields
    let ComparableFieldsOutput {
        comparable_fields,
        issues,
        object_fields,
        scalar_fields,
    } = resolve_comparable_fields(
        &object_boolean_expression_operand.comparable_fields,
        &object_type_representation.object_type,
        boolean_expression_type_name,
        subgraph,
        scalar_boolean_expression_types,
        graphql,
        raw_boolean_expression_types,
        flags,
    )?;

    // resolve any comparable relationships
    let comparable_relationships = resolve_comparable_relationships(
        boolean_expression_type_name,
        &qualified_object_type_name,
        &object_boolean_expression_operand.comparable_relationships,
        relationships,
        subgraph,
        raw_boolean_expression_types,
    )?;

    // resolve graphql schema information
    let resolved_graphql = graphql
        .as_ref()
        .map(|object_boolean_graphql_config| {
            graphql::resolve_object_boolean_graphql(
                boolean_expression_type_name,
                object_boolean_graphql_config,
                &comparable_fields,
                scalar_boolean_expression_types,
                raw_boolean_expression_types,
                graphql_config,
                graphql_types,
            )
        })
        .transpose()?;

    Ok((
        ResolvedObjectBooleanExpressionType {
            name: boolean_expression_type_name.clone(),
            include_logical_operators: helpers::resolve_logical_operators(logical_operators),
            fields: ResolvedObjectBooleanExpressionTypeFields {
                object_fields,
                scalar_fields,
                relationship_fields: comparable_relationships,
            },
            object_type: qualified_object_type_name.clone(),
            graphql: resolved_graphql,
        },
        issues,
    ))
}

// resolve comparable relationships. More indepth checks
// should occur when resolving models, when the model source is known.
fn resolve_comparable_relationships(
    boolean_expression_type_name: &Qualified<CustomTypeName>,
    underlying_object_type_name: &Qualified<CustomTypeName>,
    comparable_relationships: &Vec<
        open_dds::boolean_expression::BooleanExpressionComparableRelationship,
    >,
    relationships: &relationships::Relationships,
    subgraph: &SubgraphName,
    raw_boolean_expression_types: &RawBooleanExpressionTypes,
) -> Result<BTreeMap<FieldName, BooleanExpressionComparableRelationship>, BooleanExpressionError> {
    let mut resolved_comparable_relationships = BTreeMap::new();

    for comparable_relationship in comparable_relationships {
        let relationship = relationships.get(
            underlying_object_type_name,
            &comparable_relationship.relationship_name,
        )?;

        match relationship {
            relationships::Relationship::Relationship(relationship) => {
                // if the relationship has provided an optional boolean_expression_type, let's
                // check it makes sense
                let target_boolean_expression_type = comparable_relationship
                    .boolean_expression_type
                    .as_ref()
                    .map(
                        |target_boolean_expression_type_name| -> Result<_, BooleanExpressionError> {
                            // create target boolean expression name
                            let target_boolean_expression_type = Qualified::new(
                                crate::helpers::relationship::get_target_subgraph(relationship)
                                    .unwrap_or(subgraph.clone()),
                                target_boolean_expression_type_name.clone(),
                            );

                            // ...and ensure it exists
                            let _raw_boolean_expression_type =
                                helpers::lookup_raw_boolean_expression(
                                    boolean_expression_type_name,
                                    &target_boolean_expression_type,
                                    raw_boolean_expression_types,
                                )?;

                            Ok(target_boolean_expression_type)
                        },
                    )
                    .transpose()?;

                resolved_comparable_relationships.insert(
                    FieldName::new(comparable_relationship.relationship_name.inner().clone()),
                    BooleanExpressionComparableRelationship {
                        relationship_name: comparable_relationship.relationship_name.clone(),
                        boolean_expression_type: target_boolean_expression_type,
                    },
                );
            }

            // If the relationship is to an unknown subgraph, skip it because we're in
            // allow unknown subgraphs mode
            relationships::Relationship::RelationshipToUnknownSubgraph => {}
        };
    }

    Ok(resolved_comparable_relationships)
}

pub struct ComparableFieldsOutput {
    comparable_fields: BTreeMap<FieldName, (ComparableFieldKind, Qualified<CustomTypeName>)>,
    object_fields: BTreeMap<FieldName, ObjectComparisonExpressionInfo>,
    scalar_fields: BTreeMap<FieldName, ComparisonExpressionInfo>,
    issues: Vec<BooleanExpressionIssue>,
}

// comparable_fields don't do much, all we can do is ensure that the other BooleanExpressionTypes
// they refer to exist
fn resolve_comparable_fields(
    comparable_fields: &Vec<BooleanExpressionComparableField>,
    object_type_representation: &object_types::ObjectTypeRepresentation,
    boolean_expression_type_name: &Qualified<CustomTypeName>,
    subgraph: &SubgraphName,
    scalar_boolean_expression_types: &BTreeMap<
        Qualified<CustomTypeName>,
        scalar_boolean_expressions::ResolvedScalarBooleanExpressionType,
    >,
    graphql: &Option<BooleanExpressionTypeGraphQlConfiguration>,
    raw_boolean_expression_types: &RawBooleanExpressionTypes,
    flags: &open_dds::flags::Flags,
) -> Result<ComparableFieldsOutput, BooleanExpressionError> {
    let mut resolved_comparable_fields = BTreeMap::new();
    let mut issues = Vec::new();
    let mut object_fields = BTreeMap::new();
    let mut scalar_fields = BTreeMap::new();

    // validate comparable fields all exist in underlying object
    for comparable_field in comparable_fields {
        let field = object_type_representation
            .fields
            .get(&comparable_field.field_name)
            .ok_or_else(
                || BooleanExpressionError::UnknownFieldInObjectBooleanExpressionType {
                    field_name: comparable_field.field_name.clone(),
                    object_boolean_expression_type: boolean_expression_type_name.clone(),
                },
            )?;

        // fields with field arguments are not allowed in boolean expressions
        if !field.field_arguments.is_empty() {
            continue;
        }

        let field_boolean_expression_type_name = Qualified::new(
            subgraph.clone(),
            comparable_field.boolean_expression_type.clone(),
        );

        // lookup the boolean expression type to check it exists
        let (_, raw_boolean_expression_type) = helpers::lookup_raw_boolean_expression(
            boolean_expression_type_name,
            &field_boolean_expression_type_name,
            raw_boolean_expression_types,
        )?;

        // get type of field
        let field_type = field.field_type.get_underlying_type_name();

        // get type underlying boolean expression
        let (field_kind, boolean_expression_underlying_type) = match &raw_boolean_expression_type
            .operand
        {
            BooleanExpressionOperand::Object(BooleanExpressionObjectOperand { r#type, .. })
            | BooleanExpressionOperand::ObjectAggregate(
                BooleanExpressionObjectAggregateOperand { r#type, .. },
            ) => {
                let field_kind = match field.field_type.underlying_type {
                    QualifiedBaseType::List(_) => ComparableFieldKind::Array,
                    QualifiedBaseType::Named(_) => ComparableFieldKind::Object,
                };
                (field_kind, TypeName::Custom(r#type.clone()))
            }
            BooleanExpressionOperand::Scalar(BooleanExpressionScalarOperand { r#type, .. })
            | BooleanExpressionOperand::ScalarAggregate(
                BooleanExpressionScalarAggregateOperand { r#type, .. },
            ) => (ComparableFieldKind::Scalar, r#type.clone()),
        };
        if let QualifiedBaseType::List(_) = field.field_type.underlying_type {
            if field_kind == ComparableFieldKind::Scalar {
                issues.push(
                    BooleanExpressionIssue::BooleanExpressionArrayFieldComparedWithScalarType {
                        field_name: comparable_field.field_name.clone(),
                        boolean_expression_type_name: field_boolean_expression_type_name,
                    },
                );
                continue;
            }
        }

        let qualified_boolean_expression_type =
            mk_qualified_type_name(&boolean_expression_underlying_type, subgraph);

        // ensure the two types are the same
        if qualified_boolean_expression_type != *field_type {
            return Err(BooleanExpressionError::FieldTypeMismatch {
                field_boolean_expression_type_name: field_boolean_expression_type_name.clone(),
                field_name: comparable_field.field_name.clone(),
                field_type: field_type.clone(),
                underlying_type: qualified_boolean_expression_type,
            });
        }

        resolved_comparable_fields.insert(
            comparable_field.field_name.clone(),
            (field_kind, field_boolean_expression_type_name),
        );
    }

    // doing this validation when there is no graphql configuration is a breaking change, so we
    // only do it if the flag allows it

    if graphql.is_some() || flags.allow_boolean_expression_fields_without_graphql {
        for (comparable_field_name, (comparable_field_kind, comparable_field_type_name)) in
            &resolved_comparable_fields
        {
            match comparable_field_kind {
                ComparableFieldKind::Scalar => {
                    if let Some(scalar_boolean_expression_type) =
                        scalar_boolean_expression_types.get(comparable_field_type_name)
                    {
                        let operator_mapping = resolve_operator_mapping_for_scalar_type(
                            &scalar_boolean_expression_type.data_connector_operator_mappings,
                        );

                        // Register scalar comparison field only if it contains non-zero operators.
                        if !scalar_boolean_expression_type
                            .comparison_operators
                            .is_empty()
                            || scalar_boolean_expression_type.include_is_null
                                == scalar_boolean_expressions::IncludeIsNull::Yes
                        {
                            scalar_fields.insert(
                                comparable_field_name.clone(),
                                ComparisonExpressionInfo {
                                    object_type_name: Some(comparable_field_type_name.clone()),
                                    operators: scalar_boolean_expression_type
                                        .comparison_operators
                                        .clone(),
                                    operator_mapping,
                                },
                            );
                        };
                    }
                }
                ComparableFieldKind::Object | ComparableFieldKind::Array => {
                    // if this field isn't a scalar, let's see if it's an object instead
                    let (field_subgraph, raw_boolean_expression_type) =
                        helpers::lookup_raw_boolean_expression(
                            boolean_expression_type_name,
                            comparable_field_type_name,
                            raw_boolean_expression_types,
                        )?;

                    if let BooleanExpressionOperand::Object(object_operand) =
                        &raw_boolean_expression_type.operand
                    {
                        object_fields.insert(
                            comparable_field_name.clone(),
                            ObjectComparisonExpressionInfo {
                                field_kind: match comparable_field_kind {
                                    ComparableFieldKind::Array => ObjectComparisonKind::Array,
                                    ComparableFieldKind::Object => ObjectComparisonKind::Object,
                                    ComparableFieldKind::Scalar => unreachable!(),
                                },
                                object_type_name: comparable_field_type_name.clone(),
                                underlying_object_type_name: Qualified::new(
                                    (*field_subgraph).clone(),
                                    object_operand.r#type.clone(),
                                ),
                            },
                        );
                    }
                }
            }
        }
    }

    Ok(ComparableFieldsOutput {
        object_fields,
        scalar_fields,
        comparable_fields: resolved_comparable_fields,
        issues,
    })
}

fn resolve_operator_mapping_for_scalar_type(
    data_connector_operator_mappings: &BTreeMap<
        Qualified<DataConnectorName>,
        DataConnectorOperatorMapping,
    >,
) -> BTreeMap<Qualified<DataConnectorName>, OperatorMapping> {
    let mut operator_mapping = BTreeMap::new();

    for (data_connector_name, data_connector_operator_mapping) in data_connector_operator_mappings {
        operator_mapping.insert(
            data_connector_name.clone(),
            OperatorMapping(data_connector_operator_mapping.operator_mapping.clone()),
        );
    }

    operator_mapping
}
