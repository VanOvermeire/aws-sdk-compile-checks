# AWS SDK Compile Checks

Checks for the presence of required attributes when using calling AWS SDK client methods for some 300 AWS crates and 9000 methods.
By default, the AWS SDK will panic at runtime when required properties are missing.
With this macro, we shift to the left, failing at compile-time.

## Install

Add the macro with the following command

```
cargo add aws-sdk-compile-checks
```

## Usage

Add the `#[required_props]` attribute to functions that are using an AWS client.
For example:

```rust
#[required_props]
async fn example() -> Result<()> {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let sqs_client = aws_sdk_sqs::Client::new(&aws_config);
    sqs_client.send_message()
        // where is the queue_url?
        .message_body(create_message())
        .send()
        .await
        .with_context(|| "call to sqs failed")?;
    Ok(())
}
```

In the above example, you will get a compile time error complaining that `queue_url()`, which required, is missing.

You can also add the attribute to `impl` blocks. Here's a valid (non-throwing) example:

```rust
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
#[required_props(sdk = sqs)]
async fn do_call() -> Result<()> {
    // same content
}
```

More importantly, specifying your SDK(s) is required when there is an overlap in method names. 
I.e. when more than one SDK has the method name, and the required properties differ from each other.
For example, both `connectparticipant` and `sqs` have a `send_message` method.
In that case, the SDK might not be able to identify the right SDK, and will ask you to be more specific by adding an `sdk` parameter.

## Limitations

When used without additional arguments, this macro tries to make an _educated guess_ as to what specific SDK client is used, by looking at things like the signature, type, and naming.
Plus, the method name is often unique, which means we know perfectly well what the required properties are.

But it's heuristics, meaning things can still go wrong:
- we might think something is an SDK call, when it is not
- we might think something is not an SDK call, when it is
- we might misidentify the method
- ...

The false positives and negatives are unavoidable in sufficiently complex use cases.
They can be mitigated by improving the heuristics, but in some cases you will have to take action yourself:
- split off code that was misidentified as an SDK call, or remove the macro
- pick names that help the macro identify the correct client (e.g. `sqs_client` instead of `client`)
- specify the SDKs that need to be checked with `sdk =` (see the examples section above)

In theory, the required properties could change over time, which would mean the macro should somehow take into account what version of the AWS SDK you are running.
But in practice this should be extremely rare for backwards-compatibility reasons.
False negatives can occur when new methods are added to the SDKs that are not yet in the list maintained by this macro.

Finally, _several crates are currently not included_:

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

In some cases, I thought they were less important in general, or perhaps less in need of these compile time checks.
And more crates and methods mean more space and time requirements.
On request, crates from the above list can be added with fairly little effort.

## TODO

- Inline documentation
- Less owning of stuff
- More tests
- Refactoring of internals
- When multiple clients are present and have the same method name, thrown an error?
