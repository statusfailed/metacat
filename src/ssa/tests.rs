use super::*;
use crate::category::core::{Dtype, NdArrayType, ScalarOp, Shape, TensorOp};
use open_hypergraphs::lax::OpenHypergraph;

fn print_ssa(ssa: &[SSA<NdArrayType, TensorOp>]) {
    println!(
        "{}",
        ssa.iter()
            .map(|ssa| format!("{ssa}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn test_simple_operation_ssa() {
    // Start with a simpler test - just one Map operation
    let input_type = NdArrayType {
        shape: Shape(vec![2, 2]),
        dtype: Dtype::F32,
    };
    let output_type = NdArrayType {
        shape: Shape(vec![2, 2]),
        dtype: Dtype::F32,
    };

    // Build the open hypergraph
    let mut graph = OpenHypergraph::empty();

    // Create input and output nodes
    let input_node = graph.new_node(input_type);
    let output_node = graph.new_node(output_type);

    // Add a simple Map operation (negation)
    let _ = graph.new_edge(
        TensorOp::Map(ScalarOp::Neg),
        lax::Hyperedge {
            sources: vec![input_node],
            targets: vec![output_node],
        },
    );

    // Set global sources and targets
    graph.sources = vec![input_node];
    graph.targets = vec![output_node];

    // Convert to strict form for SSA decomposition
    let strict_graph = graph.to_strict();

    // Decompose to SSA
    let ssa_form = ssa(strict_graph).expect("cycle found");

    // Print the SSA
    println!("SSA Decomposition:");
    print_ssa(&ssa_form);

    // Basic assertions
    assert_eq!(ssa_form.len(), 1); // Should have 1 operation
}

#[test]
fn test_matmul_and_pointwise_sum_ssa() {
    // Create tensor types
    let matrix_a_type = NdArrayType {
        shape: Shape(vec![2, 3]),
        dtype: Dtype::F32,
    };
    let matrix_b_type = NdArrayType {
        shape: Shape(vec![3, 4]),
        dtype: Dtype::F32,
    };
    let result_matmul_type = NdArrayType {
        shape: Shape(vec![2, 4]),
        dtype: Dtype::F32,
    };
    let vector_type = NdArrayType {
        shape: Shape(vec![2, 4]),
        dtype: Dtype::F32,
    };
    let final_result_type = NdArrayType {
        shape: Shape(vec![2, 4]),
        dtype: Dtype::F32,
    };

    // Build the open hypergraph
    let mut graph = OpenHypergraph::empty();

    // Create input nodes
    let a_node = graph.new_node(matrix_a_type);
    let b_node = graph.new_node(matrix_b_type);
    let c_node = graph.new_node(vector_type);

    // Create intermediate result node for matmul
    let matmul_result_node = graph.new_node(result_matmul_type);

    // Create final result node
    let final_result_node = graph.new_node(final_result_type);

    // Add matrix multiplication edge (Contract operation)
    let _matmul_edge = graph.new_edge(
        TensorOp::MatMul,
        lax::Hyperedge {
            sources: vec![a_node, b_node],
            targets: vec![matmul_result_node],
        },
    );

    // Add pointwise sum edge (Map operation)
    let _sum_edge = graph.new_edge(
        TensorOp::Map(ScalarOp::Add),
        lax::Hyperedge {
            sources: vec![matmul_result_node, c_node],
            targets: vec![final_result_node],
        },
    );

    // Set global sources and targets
    graph.sources = vec![a_node, b_node, c_node];
    graph.targets = vec![final_result_node];

    // Convert to strict form for SSA decomposition
    let strict_graph = graph.to_strict();

    // Decompose to SSA
    let ssa_form = ssa(strict_graph).expect("cycle found");

    // Print the SSA
    println!("SSA Decomposition:");
    print_ssa(&ssa_form);

    // Basic assertions
    assert_eq!(ssa_form.len(), 2); // Should have 2 operations: matmul + pointwise sum
}
