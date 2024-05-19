use aws_config::BehaviorVersion;
use aws_sdk_compile_checks_macro::required_props;

#[required_props]
async fn do_call() {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let sqs_client = aws_sdk_sqs::Client::new(&aws_config);
    sqs_client.send_message()
        .send()
        .await
        .expect("Call to succeed");
}

fn main() {}
