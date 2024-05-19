use aws_sdk_compile_checks_macro::required_props;
use aws_sdk_amplify::Client;

#[required_props]
async fn do_call(client: Client) {
    client.list_artifacts()
        .send()
        .await
        .expect("Call to succeed");
}

fn main() {}
