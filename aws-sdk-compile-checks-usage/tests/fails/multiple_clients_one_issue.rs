use aws_sdk_compile_checks_macro::required_props;

#[required_props]
async fn dynamo_and_sqs(sqs_client: aws_sdk_sqs::Client, dynamodb_client: aws_sdk_dynamodb::Client) {
    let _ = sqs_client.receive_message()
        .queue_url("")
        .send()
        .await;
    let _ = dynamodb_client
        .create_global_table()
        .global_table_name("...")
        .send()
        .await;
}

fn main() {}
