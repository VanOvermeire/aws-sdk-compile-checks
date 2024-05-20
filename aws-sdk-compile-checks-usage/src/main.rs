#![allow(dead_code)]
#![allow(unused)]

use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::types::Replica;
use aws_sdk_sagemaker::types::ServiceCatalogProvisioningDetails;
use aws_sdk_compile_checks_macro::required_props;
use aws_sdk_sqs::Client;

// these are normal functions and should not error

#[required_props]
async fn function_b() {
    println!("Doing nothing");
}

async fn send_message() {
    println!("Sending");
}

#[required_props]
async fn function_a() -> i32 {
    send_message().await;
    function_b().await;
    42
}

// this looks like an SDK call, but it isn't

struct ReceiveClient {}

impl ReceiveClient {
    async fn receive_message(&self) {}
}

#[required_props]
async fn call_with_unused_param(sqs_client: Client, other_client: ReceiveClient) {
    let _ = other_client.receive_message().await;
}

// these work because of the sqs prefix allows us to guess what SDK we are dealing with

#[required_props]
async fn another_call() {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let sqs_client = Client::new(&aws_config);
    sqs_client
        .receive_message()
        .queue_url("something")
        .send()
        .await
        .expect("Call to succeed");
}

#[required_props]
async fn call_with_param(queue_url: &str, sqs_client: aws_sdk_sqs::Client) {
    let _ = sqs_client.receive_message().queue_url(queue_url).send().await;
}

#[required_props]
async fn call_with_param_sdk_clear_from_param(queue_url: &str, client: aws_sdk_sqs::Client) {
    let _ = client.send_message().queue_url(queue_url).message_body("message").send().await;
}

#[required_props]
async fn unfinished_call() {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let sqs_client = Client::new(&aws_config);
    let _fut = sqs_client.receive_message().queue_url("something").send();
}

struct AwsClientPrefix {
    sqs_client: Client,
}

impl AwsClientPrefix {
    async fn new() -> Self {
        let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        Self {
            sqs_client: Client::new(&aws_config),
        }
    }

    #[required_props]
    async fn call(&self) {
        let _ = self.sqs_client.receive_message().queue_url("something").send().await;
    }
}

struct AnotherClient {
    sqs_client: Client,
}

impl AnotherClient {
    #[required_props]
    async fn a_call(&self) {
        let _ = self.sqs_client
            .send_message()
            .queue_url("something")
            .message_body("message")
            .send()
            .await;
    }
}

// these work because we specify the SDK

#[required_props(sdk = sqs)]
async fn do_call() {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = aws_sdk_sqs::Client::new(&aws_config);
    client
        .send_message()
        .queue_url("something")
        .message_body("message")
        .send()
        .await
        .expect("Call to succeed");
}

struct AwsClientNoPrefix {
    client: Client,
}

impl AwsClientNoPrefix {
    async fn new() -> Self {
        let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        Self {
            client: Client::new(&aws_config),
        }
    }

    #[required_props(sdk = sqs)]
    async fn call(&self) {
        let _ = self.client
            .receive_message()
            .queue_url("something")
            .send()
            .await;
    }
}

#[required_props(sdk = sqs,sns,ses)]
async fn call_with_sqs_client_not_sns_or_ses(client: Client) {
    let _ = client.send_message()
        .queue_url("")
        .message_body("")
        .send()
        .await;
}

// multiple clients
// (not fully supported)

#[required_props]
async fn dynamo_and_sqs(sqs_client: aws_sdk_sqs::Client, dynamodb_client: aws_sdk_dynamodb::Client) {
    let _ = sqs_client.receive_message()
        .queue_url("")
        .send()
        .await;
    let _ = dynamodb_client
        .create_global_table()
        .global_table_name("...")
        .replication_group(Replica::builder().region_name("eu-west-1").build())
        .send()
        .await;
}

#[required_props]
async fn evidently_and_sagemaker(evidently_client: aws_sdk_evidently::Client, sagemaker_client: aws_sdk_sagemaker::Client) {
    let _ = evidently_client.create_project()
        .name("name")
        .send()
        .await;
    let _ = sagemaker_client.create_project()
        .project_name("name")
        .service_catalog_provisioning_details(ServiceCatalogProvisioningDetails::builder().build())
        .send()
        .await;
}

#[required_props(sdk = evidently,sagemaker)]
async fn evidently_and_sagemaker_with_selected_sdks(evidently_client: aws_sdk_evidently::Client, sagemaker_client: aws_sdk_sagemaker::Client) {
    let _ = evidently_client.create_project()
        .name("name")
        .send()
        .await;
    let _ = sagemaker_client.create_project()
        .project_name("name")
        .service_catalog_provisioning_details(ServiceCatalogProvisioningDetails::builder().build())
        .send()
        .await;
}

// ideally, this would not cause a compile error (though on the other hand, why add the attribute to a call that is not an SDK call?)

// struct SomeClient {}
//
// impl SomeClient {
//     async fn send_message(&self) {}
// }
//
// #[required_props]
// async fn not_sdk_call() {
//     let client = SomeClient {}; // this shows it's not an SDK client
//     client.send_message()
//         .await;
// }

fn main() {}
