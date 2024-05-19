use aws_sdk_compile_checks_macro::required_props;

#[required_props(sdk = sqs/s3)]
async fn do_call() {
    // irrelevant
}

fn main() {}
