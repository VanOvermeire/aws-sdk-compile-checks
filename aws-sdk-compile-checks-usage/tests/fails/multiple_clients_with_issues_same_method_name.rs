use aws_sdk_compile_checks_macro::required_props;

#[required_props]
async fn evidently(evidently_client: aws_sdk_evidently::Client, rekognition_client: aws_sdk_rekognition::Client) {
    let _ = evidently_client.create_project()
        .send()
        .await;
    let _ = rekognition_client.create_project()
        .send()
        .await;
}

fn main() {}
