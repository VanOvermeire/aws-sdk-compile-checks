# AWS SDK Compile Checks

Checks for the presence of required attributes when using calling AWS SDK client methods for some 300 AWS crates and 9000 methods.

By default, the AWS SDK will panic at runtime when required properties are missing.
With this macro, we shift to the left, failing at compile time.

## Install

Add the macro to your dependencies with the following command:

```ignore
cargo add aws-sdk-compile-checks
```

## Usage

After adding the crate to your dependencies, you can use the `#[required_props]` attribute to annotate functions that use an AWS client.

For example:

```rust ignore
use aws_sdk_compile_checks_macro::required_props;
use aws_sdk_sqs::config::BehaviorVersion;

#[required_props]
async fn example() -> Result<(), String> {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let sqs_client = aws_sdk_sqs::Client::new(&aws_config);
    sqs_client.send_message()
        // missing queue url
        .message_body("some message")
        .send()
        .await;
    Ok(())
}
```

In the above example, you will get a compile time error complaining that `queue_url()`, which required, is missing.

You can also add the attribute to `impl` blocks. The following example will compile since it has all the required properties:

```rust
use aws_sdk_compile_checks_macro::required_props;
use aws_sdk_sqs::Client;

struct AwsClientPrefix {
    sqs_client: Client,
}

impl AwsClientPrefix {
    #[required_props]
    async fn call(&self) {
        let _ = self.sqs_client
            .receive_message()
            .queue_url("something")
            .send()
            .await;
    }
}
```

You can specify SDKs. This might speed up the search process a little bit.

```rust
use aws_sdk_compile_checks_macro::required_props;
use aws_sdk_sqs::Client;

#[required_props(sdk = sqs)]
async fn do_call(sqs_client: Client) {
    let _ = sqs_client
        .receive_message()
        .queue_url("something")
        .send()
        .await;
}
```

Specifying your SDK(s) is required when there is an overlap in method names.
I.e. when more than one SDK has a given method name, and the required properties differ.
For example, both `connectparticipant` and `sqs` have a `send_message` method.
In that case, the macro might not be able to identify the right SDK. If that's the case, it will ask you to be more specific.

## Limitations

When used without additional arguments, the macro tries to make an _educated guess_ as to what specific SDK client is used, by looking at things like the signature, type, and naming.
Plus, as the method name is often unique, we often have only one list of required properties.

But it's all heuristics, so things can go wrong:
- we might think something is an SDK call, when it is not
- we might think something is not an SDK call, when it is
- we might misidentify the method
- ...

The false positives and negatives are unavoidable in sufficiently complex use cases.
They can be mitigated by improving the heuristics, but in some cases you will have to take action yourself:
- split off code that was misidentified as an SDK call
- pick names that help the macro identify the correct client (e.g. `sqs_client` instead of `client`)
- specify the SDKs that need to be checked with `sdk =` (see the 'Usage' section above)

In theory, the required properties could evolve over time, which would mean the macro should take into account what version of the AWS SDK you are running.
But since this is a breaking change for existing properties, this should be very rare.
False negatives can occur when new methods are added to the SDKs that are not yet in the list maintained by this macro though.

## PRs etc.

Pull requests, comments, suggestions... are welcome.

## TODO

- Less owning of stuff
- Some refactoring
- Set up GitHub actions checks
- Pick another example sdk in 'usage' that's faster than sagemaker?

## Crates that are not yet included

Some 30 crates are currently not included, you can find a list below.

In some cases, I thought these crates were less important, or had less use for compile time checks.
And more crates and methods mean more space and time requirements.
On request, crates from this list can be added.

- aws_sdk_finspace
- bcmdataexports
- chime
- chimesdkidentity
- chimesdkmediapipelines
- chimesdkmeetings
- chimesdkmessaging
- chimesdkvoice
- cloudsearchdomain
- databasemigration
- ivschat
- licensemanager
- licensemanagerlinuxsubscriptions
- licensemanagerusersubscriptions
- migrationhub
- migrationhubconfig
- migrationhuborchestrator
- migrationhubrefactorspaces
- migrationhubstrategy
- rbin
- simspaceweaver
- tnb
- wellarchitected
- wisdom
- workdocs
- worklink
- workmail
- workmailmessageflow
- workspaces
- workspacesthinclient
- workspacesweb
