use aws_config::BehaviorVersion;
use aws_sdk_compile_checks_macro::required_props;

#[required_props]
async fn do_call() {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let _something = -3_i32.abs();
    let sqs_client = aws_sdk_sqs::Client::new(&aws_config);
    let _calculate = 3 + 2;
    sqs_client.send_message()
        .message_body(create_message())
        .send()
        .await
        .expect("Call to succeed");
    let _something = -3_i32.abs();
}

fn create_message<'a>() -> &'a str {
    "some message"
}

fn main() {}
