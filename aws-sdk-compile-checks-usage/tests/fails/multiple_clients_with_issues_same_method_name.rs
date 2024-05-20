use aws_sdk_compile_checks_macro::required_props;

#[required_props]
async fn evidently(evidently_client: aws_sdk_evidently::Client, sagemaker_client: aws_sdk_sagemaker::Client) {
    let _ = evidently_client.create_project()
        .send()
        .await;
    let _ = sagemaker_client.create_project()
        .send()
        .await;
}

fn main() {}
