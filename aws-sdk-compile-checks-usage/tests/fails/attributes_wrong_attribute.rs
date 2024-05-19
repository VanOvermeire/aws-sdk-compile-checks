use aws_sdk_compile_checks_macro::required_props;

#[required_props(fake)]
async fn do_call() {
    // irrelevant
}

fn main() {}
