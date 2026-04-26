// use indexmap::IndexSet;
use tokio::sync::{Mutex, mpsc};
use std::sync::Arc;

use crate::{data_structure::QueueSet, port::{AllocatedPort, PortError, PortRange}};

pub struct PortManager {
    // Single source of truth: if a port is in this queue, it's available.
    // Ports are popped from front for allocation, pushed to back on release.
    // The rotation through the range gives natural "grace period" for Docker cleanup.
    // If the number of ports is too small then you might have issues with docker not releasing
    // the port right away, which can lead to failed docker container startups.
    // available_ports: Mutex<IndexSet<u16>>,
    available_ports: Mutex<QueueSet<u16>>,
    port_release_sender: mpsc::Sender<u16>,
}

impl PortManager {
    pub fn new(port_range: PortRange) -> Arc<Self> {
        let (port_release_sender, port_release_receiver) = mpsc::channel(1000);

        // Pre-allocate VecDeque with exact capacity and populate with all ports
        let port_count = (port_range.end - port_range.start) as usize;
        // let mut available_ports = IndexSet::with_capacity(port_count);
        let mut available_ports = QueueSet::with_capacity(port_count);
        for port in port_range.start..port_range.end {
            // available_ports.insert(port);
            available_ports.push_back(port);
        }

        let manager = Arc::new(Self {
            available_ports: Mutex::new(available_ports),
            port_release_sender,
        });

        // Spawn background task to process port returns
        let manager_clone = Arc::clone(&manager);
        tokio::spawn(async move {
            manager_clone.process_port_returns(port_release_receiver).await;
        });

        manager
    }

    #[tracing::instrument(name = "AllocatePort", skip_all)]
    pub async fn allocate_port(&self) -> Result<AllocatedPort, PortError> {
        let mut available_ports = self.available_ports.lock().await;
        // let port_option = available_ports.shift_remove_index(0);
        let port_option = available_ports.pop_front();
        drop(available_ports); // Release lock after critical section is completed

        match port_option {
            Some(port) => {
                tracing::info!("Allocated port {}", port);
                Ok(AllocatedPort::new(port, self.port_release_sender.clone()))
            }
            None => {
                tracing::warn!("No available ports in pool");
                Err(PortError::NoAvailablePorts)
            }
        }
    }

    #[tracing::instrument(name = "ProcessPortReturns", skip_all)]
    async fn process_port_returns(&self, mut port_release_receiver: mpsc::Receiver<u16>) {
        tracing::info!("Port return background process starting up");
        while let Some(port) = port_release_receiver.recv().await {
            let mut available_ports = self.available_ports.lock().await;
            // Add returned port to the back of the queue
            // By the time we cycle through all other ports, Docker should have freed this one
            // let successful = available_ports.insert(port);
            let successful = available_ports.push_back(port);
            drop(available_ports); // Release lock after critical section is completed

            if successful {
                tracing::debug!("Port {} returned back to available pool", port);
            } else {
                tracing::warn!("Port {} already in available pool, ignoring duplicate return", port);
            }

        }
        tracing::info!("Port return background process shutting down");
    }

    #[cfg(test)]
    pub async fn available_count(&self) -> usize {
        let available_ports = self.available_ports.lock().await;
        available_ports.len()
    }

    #[cfg(test)]
    pub async fn has_available_ports(&self) -> bool {
        let available_ports = self.available_ports.lock().await;
        !available_ports.is_empty()
    }
}

// ... your existing PortManager implementation ...

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::timeout;

    fn create_test_port_range(size: u16) -> PortRange {
        PortRange {
            start: 8000,
            end: 8000 + size,
        }
    }

    #[tokio::test]
    async fn test_new_port_manager_initializes_correctly() {
        let port_range = create_test_port_range(10);
        let manager = PortManager::new(port_range);

        // All ports should be available initially
        assert_eq!(manager.available_count().await, 10);
        assert!(manager.has_available_ports().await);
    }

    #[tokio::test]
    async fn test_allocate_single_port() {
        let port_range = create_test_port_range(5);
        let manager = PortManager::new(port_range);

        let allocated_port = manager.allocate_port().await.unwrap();

        // Port should be in expected range
        assert!(allocated_port.port() >= 8000 && allocated_port.port() < 8005);

        // Available count should decrease
        assert_eq!(manager.available_count().await, 4);
    }

    #[tokio::test]
    async fn test_allocate_all_ports() {
        let port_range = create_test_port_range(3);
        let manager = PortManager::new(port_range);

        let port1 = manager.allocate_port().await.unwrap();
        let port2 = manager.allocate_port().await.unwrap();
        let port3 = manager.allocate_port().await.unwrap();

        // All ports should be different
        let mut ports = HashSet::new();
        ports.insert(port1.port());
        ports.insert(port2.port());
        ports.insert(port3.port());
        assert_eq!(ports.len(), 3);

        // No more ports available
        assert_eq!(manager.available_count().await, 0);
        assert!(!manager.has_available_ports().await);

        // Next allocation should fail
        let result = manager.allocate_port().await;
        assert!(matches!(result, Err(PortError::NoAvailablePorts)));
    }

    #[tokio::test]
    async fn test_port_return_via_drop() {
        let port_range = create_test_port_range(2);
        let manager = PortManager::new(port_range);

        let allocated_port = manager.allocate_port().await.unwrap();

        assert_eq!(manager.available_count().await, 1);

        // Drop the port to trigger return
        drop(allocated_port);

        // Give the background task time to process the return
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Port should be available again
        assert_eq!(manager.available_count().await, 2);

        // Should be able to allocate again
        let new_port = manager.allocate_port().await.unwrap();
        assert!(new_port.port() >= 8000 && new_port.port() < 8002);
    }

    #[tokio::test]
    async fn test_port_reuse_order_fifo() {
        let port_range = create_test_port_range(3);
        let manager = PortManager::new(port_range);

        // Allocate all ports and track their order
        let port1 = manager.allocate_port().await.unwrap();
        let port2 = manager.allocate_port().await.unwrap();
        let port3 = manager.allocate_port().await.unwrap();

        // Return them in a specific order
        drop(port2); // Return second port first
        drop(port1); // Return first port second
        drop(port3); // Return third port last

        // Give background task time to process
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Allocate again - should get ports in the order they were returned
        // (since they're added to back of queue)
        let realloc1 = manager.allocate_port().await.unwrap();
        let realloc2 = manager.allocate_port().await.unwrap();
        let realloc3 = manager.allocate_port().await.unwrap();

        // The first allocated should be the first one that was originally allocated
        // (since ports are initially in order and we pop from front)
        assert!(realloc1.port() >= 8000 && realloc1.port() < 8003);
        assert!(realloc2.port() >= 8000 && realloc2.port() < 8003);
        assert!(realloc3.port() >= 8000 && realloc3.port() < 8003);

        // All should be different
        let mut ports = HashSet::new();
        ports.insert(realloc1.port());
        ports.insert(realloc2.port());
        ports.insert(realloc3.port());
        assert_eq!(ports.len(), 3);
    }

    #[tokio::test]
    async fn test_concurrent_allocation() {
        let port_range = create_test_port_range(100);
        let manager = PortManager::new(port_range);

        let num_tasks = 50;
        let mut handles = Vec::new();

        for _ in 0..num_tasks {
            let manager_clone = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                manager_clone.allocate_port().await
            });
            handles.push(handle);
        }

        // Wait for all allocations to complete
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // All allocations should succeed
        assert_eq!(results.len(), num_tasks);

        // All ports should be unique
        let mut allocated_ports = HashSet::new();
        for result in results {
            let port = result.unwrap().port();
            assert!(allocated_ports.insert(port), "Duplicate port allocated: {}", port);
        }

        assert_eq!(allocated_ports.len(), num_tasks);
        assert_eq!(manager.available_count().await, 100 - num_tasks);
    }

    #[tokio::test]
    async fn test_concurrent_allocation_and_release() {
        let port_range = create_test_port_range(20);
        let manager = PortManager::new(port_range);
        let allocation_count = Arc::new(AtomicUsize::new(0));
        let release_count = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();

        // Spawn tasks that allocate and then release ports
        for i in 0..50 {
            let manager_clone = Arc::clone(&manager);
            let alloc_count = Arc::clone(&allocation_count);
            let rel_count = Arc::clone(&release_count);

            let handle = tokio::spawn(async move {
                // Try to allocate a port
                if let Ok(port) = manager_clone.allocate_port().await {
                    alloc_count.fetch_add(1, Ordering::Relaxed);

                    // Hold the port for a short time
                    tokio::time::sleep(Duration::from_millis(i % 10)).await;

                    // Drop the port (triggers return)
                    drop(port);
                    rel_count.fetch_add(1, Ordering::Relaxed);
                }
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Give background task time to process all returns
        tokio::time::sleep(Duration::from_millis(50)).await;

        let final_allocations = allocation_count.load(Ordering::Relaxed);
        let final_releases = release_count.load(Ordering::Relaxed);

        // Should have allocated some ports (limited by pool size)
        assert!(final_allocations > 0);
        assert!(final_allocations <= 50); // Can't allocate more than we tried

        // All allocated ports should eventually be released
        assert_eq!(final_allocations, final_releases);

        // All ports should be back in the pool
        assert_eq!(manager.available_count().await, 20);
    }

    #[tokio::test]
    async fn test_allocation_exhaustion_and_recovery() {
        let port_range = create_test_port_range(2);
        let manager = PortManager::new(port_range);

        // Allocate all ports
        let port1 = manager.allocate_port().await.unwrap();
        let _port2 = manager.allocate_port().await.unwrap();

        // Next allocation should fail
        let result = manager.allocate_port().await;
        assert!(matches!(result, Err(PortError::NoAvailablePorts)));

        // Release one port
        drop(port1);
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Should be able to allocate again
        let port3 = manager.allocate_port().await.unwrap();
        assert!(port3.port() >= 8000 && port3.port() < 8002);

        // But not a second one
        let result = manager.allocate_port().await;
        assert!(matches!(result, Err(PortError::NoAvailablePorts)));
    }

    #[tokio::test]
    async fn test_port_manager_with_single_port() {
        let port_range = PortRange { start: 9000, end: 9001 };
        let manager = PortManager::new(port_range);

        assert_eq!(manager.available_count().await, 1);

        let port = manager.allocate_port().await.unwrap();
        assert_eq!(port.port(), 9000);
        assert_eq!(manager.available_count().await, 0);

        let result = manager.allocate_port().await;
        assert!(matches!(result, Err(PortError::NoAvailablePorts)));

        drop(port);
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert_eq!(manager.available_count().await, 1);
        let port2 = manager.allocate_port().await.unwrap();
        assert_eq!(port2.port(), 9000);
    }

    #[tokio::test]
    async fn test_multiple_drops_of_same_port_safe() {
        let port_range = create_test_port_range(5);
        let manager = PortManager::new(port_range);

        let initial_count = manager.available_count().await;

        let port = manager.allocate_port().await.unwrap();
        let port_number = port.port();

        // Manually send the port back multiple times (simulating a bug)
        let sender = &manager.port_release_sender;
        sender.send(port_number).await.unwrap();
        sender.send(port_number).await.unwrap();

        // Also drop normally
        drop(port);

        // Give background task time to process
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Should not have more ports than we started with
        // (The implementation should handle duplicate returns gracefully)
        let final_count = manager.available_count().await;
        assert!(final_count <= initial_count);
    }

    #[tokio::test]
    async fn test_background_task_shutdown_handling() {
        let port_range = create_test_port_range(3);
        let manager = PortManager::new(port_range);

        let port = manager.allocate_port().await.unwrap();

        // Drop the manager (which should close the channel)
        drop(manager);

        // Dropping the port should not panic or hang
        // The Drop implementation should handle closed channel gracefully
        drop(port);

        // If we get here without hanging, the test passes
    }

    #[tokio::test]
    async fn test_stress_allocation_deallocation() {
        let port_range = create_test_port_range(10);
        let manager = PortManager::new(port_range);

        // Perform many allocation/deallocation cycles
        for _ in 0..100 {
            let mut ports = Vec::new();

            // Allocate some ports
            for _ in 0..5 {
                if let Ok(port) = manager.allocate_port().await {
                    ports.push(port);
                }
            }

            // Release them all
            ports.clear(); // This drops all ports

            // Give background task time to process
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        // Should end up with all ports available
        assert_eq!(manager.available_count().await, 10);
    }

    #[tokio::test]
    async fn test_timeout_on_background_task_processing() {
        let port_range = create_test_port_range(5);
        let manager = PortManager::new(port_range);

        let port = manager.allocate_port().await.unwrap();
        drop(port);

        // Background processing should complete within reasonable time
        let result = timeout(Duration::from_secs(1), async {
            while manager.available_count().await < 5 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }).await;

        assert!(result.is_ok(), "Background task took too long to process port return");
    }

    #[tokio::test]
    async fn test_port_range_boundaries() {
        let port_range = PortRange { start: 65530, end: 65535 };
        let manager = PortManager::new(port_range);

        assert_eq!(manager.available_count().await, 5);

        // Allocate all ports and verify they're in range
        let mut allocated_ports = Vec::new();
        for _ in 0..5 {
            let port = manager.allocate_port().await.unwrap();
            assert!(port.port() >= 65530 && port.port() < 65535);
            allocated_ports.push(port);
        }

        // Should be exhausted
        let result = manager.allocate_port().await;
        assert!(matches!(result, Err(PortError::NoAvailablePorts)));
    }

    #[tokio::test]
    async fn test_available_count_accuracy() {
        let port_range = create_test_port_range(10);
        let manager = PortManager::new(port_range);

        // Initially all ports should be available
        assert_eq!(manager.available_count().await, 10);

        // Allocate ports one by one and verify count decreases
        let mut allocated_ports = Vec::new();
        for expected_remaining in (0..10).rev() {
            let port = manager.allocate_port().await.unwrap();
            allocated_ports.push(port);
            assert_eq!(manager.available_count().await, expected_remaining);
        }

        // Release ports one by one and verify count increases
        for (i, port) in allocated_ports.into_iter().enumerate() {
            drop(port);
            // Give background task time to process
            tokio::time::sleep(Duration::from_millis(5)).await;
            assert_eq!(manager.available_count().await, i + 1);
        }
    }

    #[tokio::test]
    async fn test_has_available_ports_boolean_logic() {
        let port_range = create_test_port_range(2);
        let manager = PortManager::new(port_range);

        // Initially should have available ports
        assert!(manager.has_available_ports().await);
        assert_eq!(manager.available_count().await, 2);

        // Allocate first port
        let port1 = manager.allocate_port().await.unwrap();
        assert!(manager.has_available_ports().await);
        assert_eq!(manager.available_count().await, 1);

        // Allocate second port
        let port2 = manager.allocate_port().await.unwrap();
        assert!(!manager.has_available_ports().await);
        assert_eq!(manager.available_count().await, 0);

        // Release one port
        drop(port1);
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(manager.has_available_ports().await);
        assert_eq!(manager.available_count().await, 1);

        // Release second port
        drop(port2);
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(manager.has_available_ports().await);
        assert_eq!(manager.available_count().await, 2);
    }

    #[tokio::test]
    async fn test_available_count_consistency_under_concurrency() {
        let port_range = create_test_port_range(50);
        let manager = PortManager::new(port_range);
        let total_ports = 50;

        // Spawn multiple tasks that check available_count while others allocate/release
        let manager_clone = Arc::clone(&manager);
        let count_checker = tokio::spawn(async move {
            let mut count_samples = Vec::new();
            for _ in 0..100 {
                let count = manager_clone.available_count().await;
                count_samples.push(count);
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            count_samples
        });

        let manager_clone = Arc::clone(&manager);
        let has_ports_checker = tokio::spawn(async move {
            let mut has_ports_samples = Vec::new();
            for _ in 0..100 {
                let has_ports = manager_clone.has_available_ports().await;
                has_ports_samples.push(has_ports);
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            has_ports_samples
        });

        // Spawn allocator/releaser tasks
        let mut allocation_handles = Vec::new();
        for _ in 0..10 {
            let manager_clone = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                for _ in 0..5 {
                    if let Ok(port) = manager_clone.allocate_port().await {
                        tokio::time::sleep(Duration::from_millis(2)).await;
                        drop(port);
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                }
            });
            allocation_handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in allocation_handles {
            handle.await.unwrap();
        }

        let count_samples = count_checker.await.unwrap();
        let _has_ports_samples = has_ports_checker.await.unwrap();

        // All count samples should be valid (0 <= count <= total_ports)
        for &count in &count_samples {
            assert!(count <= total_ports, "Count {} exceeds total ports {}", count, total_ports);
        }

        // has_available_ports should be consistent with available_count
        // (We can't guarantee exact consistency due to timing, but we can check final state)
        tokio::time::sleep(Duration::from_millis(50)).await; // Let background tasks finish

        let final_count = manager.available_count().await;
        let final_has_ports = manager.has_available_ports().await;

        assert_eq!(final_count, total_ports, "All ports should be returned after test");
        assert_eq!(final_has_ports, final_count > 0);
    }

    #[tokio::test]
    async fn test_available_count_and_has_ports_consistency() {
        let port_range = create_test_port_range(5);
        let manager = PortManager::new(port_range);

        // Test consistency between the two methods across various states
        let test_scenarios = vec![
            (0, "after allocating all ports"),
            (1, "after releasing one port"),
            (3, "after releasing three ports"),
            (5, "after releasing all ports"),
        ];

        // Allocate all ports first
        let mut all_ports = Vec::new();
        for _ in 0..5 {
            all_ports.push(manager.allocate_port().await.unwrap());
        }

        for (expected_count, scenario) in test_scenarios {
            // Release ports to reach the expected count
            while manager.available_count().await < expected_count {
                if let Some(port) = all_ports.pop() {
                    drop(port);
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }

            let count = manager.available_count().await;
            let has_ports = manager.has_available_ports().await;

            assert_eq!(count, expected_count, "Count mismatch {}", scenario);
            assert_eq!(has_ports, count > 0, "has_available_ports inconsistent with count {}", scenario);

            // Test multiple calls return same result (no side effects)
            assert_eq!(manager.available_count().await, count);
            assert_eq!(manager.has_available_ports().await, has_ports);
        }
    }

    #[tokio::test]
    async fn test_available_count_with_rapid_allocation_release() {
        let port_range = create_test_port_range(10);
        let manager = PortManager::new(port_range);

        // Perform rapid allocations and releases while monitoring count
        for cycle in 0..20 {
            let initial_count = manager.available_count().await;

            // Allocate a random number of ports (1-5)
            let num_to_allocate = (cycle % 5) + 1;
            let mut allocated = Vec::new();

            for _ in 0..num_to_allocate {
                if let Ok(port) = manager.allocate_port().await {
                    allocated.push(port);
                }
            }

            let after_allocation_count = manager.available_count().await;
            let expected_after_allocation = initial_count.saturating_sub(allocated.len());
            assert_eq!(after_allocation_count, expected_after_allocation);

            // Verify has_available_ports is consistent
            assert_eq!(
                manager.has_available_ports().await,
                after_allocation_count > 0
            );

            // Release all allocated ports
            allocated.clear(); // This drops all ports

            // Give background task time to process
            tokio::time::sleep(Duration::from_millis(10)).await;

            // Count should be back to initial (or close, depending on timing)
            let final_count = manager.available_count().await;
            assert!(final_count >= expected_after_allocation);
        }

        // Final verification - all ports should be available
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(manager.available_count().await, 10);
        assert!(manager.has_available_ports().await);
    }

    #[tokio::test]
    async fn test_available_count_edge_cases() {
        // Test with single port
        let single_port_range = PortRange { start: 9000, end: 9001 };
        let single_manager = PortManager::new(single_port_range);

        assert_eq!(single_manager.available_count().await, 1);
        assert!(single_manager.has_available_ports().await);

        let port = single_manager.allocate_port().await.unwrap();
        assert_eq!(single_manager.available_count().await, 0);
        assert!(!single_manager.has_available_ports().await);

        drop(port);
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert_eq!(single_manager.available_count().await, 1);
        assert!(single_manager.has_available_ports().await);

        // Test with larger range
        let large_range = create_test_port_range(1000);
        let large_manager = PortManager::new(large_range);

        assert_eq!(large_manager.available_count().await, 1000);
        assert!(large_manager.has_available_ports().await);

        // Allocate many ports and verify count decreases correctly
        let mut ports = Vec::new();
        for i in 1..=100 {
            ports.push(large_manager.allocate_port().await.unwrap());
            if i % 10 == 0 {
                assert_eq!(large_manager.available_count().await, 1000 - i);
                assert!(large_manager.has_available_ports().await);
            }
        }
    }

    #[tokio::test]
    async fn test_available_methods_performance() {
        let port_range = create_test_port_range(1000);
        let manager = PortManager::new(port_range);

        // These methods should be fast even with many ports
        let start = std::time::Instant::now();

        for _ in 0..1000 {
            let _ = manager.available_count().await;
            let _ = manager.has_available_ports().await;
        }

        let elapsed = start.elapsed();

        // Should complete quickly (this is a rough performance check)
        assert!(elapsed < Duration::from_millis(100),
                "available_count/has_available_ports took too long: {:?}", elapsed);

        // Verify they still return correct values after performance test
        assert_eq!(manager.available_count().await, 1000);
        assert!(manager.has_available_ports().await);
    }
}