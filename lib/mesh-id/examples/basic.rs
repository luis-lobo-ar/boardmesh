use mesh_id::{CorrelationId, new_lowercase_prefixed_id, new_prefixed_id};

fn main() {
    let request = new_prefixed_id("request");
    let session = new_lowercase_prefixed_id("session");
    let correlation = CorrelationId::new(request.clone());
    println!("request: {request}");
    println!("session: {session}");
    println!("correlation: {}", correlation.0);
}
