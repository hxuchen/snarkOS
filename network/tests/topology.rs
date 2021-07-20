// Copyright (C) 2019-2021 Aleo Systems Inc.
// This file is part of the snarkOS library.

// The snarkOS library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkOS library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkOS library. If not, see <https://www.gnu.org/licenses/>.

use snarkos_network::{topology::calculate_density, Node};
use snarkos_storage::LedgerStorage;
use snarkos_testing::{
    network::{
        test_config,
        topology::{connect_nodes, Topology},
        TestSetup,
    },
    wait_until,
};

const N: usize = 25;
const MIN_PEERS: u16 = 5;
const MAX_PEERS: u16 = 30;

async fn test_nodes(n: usize, setup: TestSetup) -> Vec<Node<LedgerStorage>> {
    let mut nodes = Vec::with_capacity(n);

    for _ in 0..n {
        nodes.push(test_node(setup.clone()).await);
    }

    nodes
}

async fn test_node(setup: TestSetup) -> Node<LedgerStorage> {
    let environment = test_config(setup);
    let node = Node::new(environment).unwrap();

    node.listen().await.unwrap();

    node
}

async fn start_nodes(nodes: &[Node<LedgerStorage>]) {
    for node in nodes {
        // Nodes are started with a slight delay to avoid having peering intervals in phase (this
        // is the hypothetical worst case scenario).
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        node.start_services().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_nodes_in_a_line() {
    let setup = TestSetup {
        consensus_setup: None,
        peer_sync_interval: 1,
        ..Default::default()
    };
    let mut nodes = test_nodes(N, setup).await;
    connect_nodes(&mut nodes, Topology::Line);
    start_nodes(&nodes).await;

    // First and Last nodes should have 1 connected peer.
    wait_until!(5, nodes.first().unwrap().peer_book.get_active_peer_count() == 1);
    wait_until!(5, nodes.last().unwrap().peer_book.get_active_peer_count() == 1);

    // All other nodes should have two.
    for node in nodes.iter().take(nodes.len() - 1).skip(1) {
        wait_until!(5, node.peer_book.get_active_peer_count() == 2);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_nodes_in_a_ring() {
    let setup = TestSetup {
        consensus_setup: None,
        peer_sync_interval: 1,
        ..Default::default()
    };
    let mut nodes = test_nodes(N, setup).await;
    connect_nodes(&mut nodes, Topology::Ring);
    start_nodes(&nodes).await;

    for node in &nodes {
        wait_until!(5, node.peer_book.get_active_peer_count() == 2);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_nodes_in_a_star() {
    let setup = TestSetup {
        consensus_setup: None,
        peer_sync_interval: 1,
        ..Default::default()
    };
    let mut nodes = test_nodes(N, setup).await;
    connect_nodes(&mut nodes, Topology::Star);
    start_nodes(&nodes).await;

    let hub = nodes.first().unwrap();
    wait_until!(10, hub.peer_book.get_active_peer_count() as usize == N - 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_nodes_in_a_mesh() {
    let setup = TestSetup {
        consensus_setup: None,
        peer_sync_interval: 5,
        min_peers: MIN_PEERS,
        max_peers: MAX_PEERS,
        ..Default::default()
    };
    let mut nodes = test_nodes(N, setup).await;
    connect_nodes(&mut nodes, Topology::Mesh);
    start_nodes(&nodes).await;

    // Set the sleep interval to 200ms to avoid locking issues.
    // Density measurement here is proportional to the min peers: if every node in the network
    // only connected to the min node count, the total number of connections would be roughly 10
    // percent of the total possible. With 25 nodes and min at 5 connections each this works out to
    // be 125/600 ≈ 0.2.
    wait_until!(15, network_density(&nodes) >= 0.2, 200);

    // Make sure the node with the largest degree centrality and smallest degree centrality don't
    // have a delta greater than the max-min peer interval allows for. This check also provides
    // some insight into whether the network is meshed in a homogeneous manner.
    wait_until!(
        15,
        degree_centrality_delta(&nodes) <= (MAX_PEERS - MIN_PEERS).into(),
        200
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn line_converges_to_mesh() {
    let setup = TestSetup {
        consensus_setup: None,
        peer_sync_interval: 1,
        min_peers: MIN_PEERS,
        max_peers: MAX_PEERS,
        ..Default::default()
    };
    let mut nodes = test_nodes(N, setup).await;
    connect_nodes(&mut nodes, Topology::Line);
    start_nodes(&nodes).await;

    wait_until!(10, network_density(&nodes) >= 0.2, 200);
    wait_until!(
        10,
        degree_centrality_delta(&nodes) <= (MAX_PEERS - MIN_PEERS).into(),
        200
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ring_converges_to_mesh() {
    let setup = TestSetup {
        consensus_setup: None,
        peer_sync_interval: 1,
        min_peers: MIN_PEERS,
        max_peers: MAX_PEERS,
        ..Default::default()
    };
    let mut nodes = test_nodes(N, setup).await;
    connect_nodes(&mut nodes, Topology::Ring);
    start_nodes(&nodes).await;

    wait_until!(10, network_density(&nodes) >= 0.2, 200);
    wait_until!(
        10,
        degree_centrality_delta(&nodes) <= (MAX_PEERS - MIN_PEERS).into(),
        200
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn star_converges_to_mesh() {
    let setup = TestSetup {
        consensus_setup: None,
        peer_sync_interval: 1,
        min_peers: MIN_PEERS,
        max_peers: MAX_PEERS,
        ..Default::default()
    };

    // A bootnode will be necessary at the center of the star for peers to get propagated.
    let hub_setup = TestSetup {
        is_bootnode: true,
        ..setup.clone()
    };

    // Create the regular nodes.
    let mut nodes = test_nodes(N - 1, setup).await;

    // Insert the bootnode at the head of the list.
    nodes.insert(0, test_node(hub_setup).await);

    connect_nodes(&mut nodes, Topology::Star);
    start_nodes(&nodes).await;

    wait_until!(15, network_density(&nodes) >= 0.2, 200);
    wait_until!(
        15,
        degree_centrality_delta(&nodes) <= (MAX_PEERS - MIN_PEERS).into(),
        200
    );
}

/// Returns the total connection count of the network.
fn total_connection_count(nodes: &[Node<LedgerStorage>]) -> u32 {
    let mut count = 0;

    for node in nodes {
        count += node.peer_book.get_connected_peer_count()
    }

    count / 2
}

// This could use the degree matrix, though as this is used extensively in tests and checked
// repeatedly until it reaches a certain value, we want to keep its calculation decoupled from the
// `NetworkMetrics`.
fn degree_centrality_delta(nodes: &[Node<LedgerStorage>]) -> u32 {
    let dc = nodes.iter().map(|node| node.peer_book.get_connected_peer_count());
    let min = dc.clone().min().unwrap();
    let max = dc.max().unwrap();

    max - min
}

/// Returns the network density.
fn network_density(nodes: &[Node<LedgerStorage>]) -> f64 {
    let connections = total_connection_count(nodes);
    calculate_density(nodes.len() as f64, connections as f64)
}
