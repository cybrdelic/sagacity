use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

// Define the maximum number of queries per batch
const MAX_BATCH_SIZE: usize = 10;
// Define the batch interval in seconds
const BATCH_INTERVAL: u64 = 5;

#[derive(Clone, Debug)]
pub struct BatchProcessor {
    sender: mpsc::Sender<String>,
}

impl BatchProcessor {
    pub fn new() -> Self {
        let (sender, mut receiver) = mpsc::channel::<String>(100);
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(BATCH_INTERVAL));
            let mut batch = Vec::new();

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !batch.is_empty() {
                            // Process the batch
                            // Replace with actual processing logic
                            println!("Processing batch of {} queries.", batch.len());
                            // Clear the batch after processing
                            batch.clear();
                        }
                    }
                    Some(query) = receiver.recv() => {
                        batch.push(query);
                        if batch.len() >= MAX_BATCH_SIZE {
                            // Process the batch
                            // Replace with actual processing logic
                            println!("Processing batch of {} queries.", batch.len());
                            // Clear the batch after processing
                            batch.clear();
                        }
                    }
                    else => break,
                }
            }
        });

        BatchProcessor { sender }
    }

    pub async fn add_query(&self, query: String) {
        if let Err(e) = self.sender.send(query).await {
            eprintln!("BatchProcessor send error: {}", e);
        }
    }
}
