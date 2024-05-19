use aws_sdk_compile_checks_macro::required_props;
use aws_sdk_lambda::Client;

#[required_props]
async fn do_call(client: Client) {
    client.tag_resource()
        .send()
        .await
        .expect("Call to succeed");
}

fn main() {}
