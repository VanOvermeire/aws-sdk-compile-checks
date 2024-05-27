use std::collections::{HashMap, HashSet};

use proc_macro2::{Ident};
use syn::{Expr, ExprMethodCall, FnArg, ItemFn, Local, Member, Pat, Signature, Type, visit};
use syn::visit::Visit;
use crate::findings::{ImproperUsage, UnknownUsage, UsageFinds};

use crate::required_properties::RequiredPropertiesMap;

const AWS_SDK_SEND: &str = "send"; // terminates calls to AWS in the SDK
const AWS_SDK_PREFIX: &str = "aws_sdk_"; // e.g. aws_sdk_sqs::Client

#[derive(Debug)]
pub(crate) struct MethodVisitor {
    clients: HashSet<Client>,
    method_calls: Vec<MethodCallWithReceiver>,
    required_props: RequiredPropertiesMap,
}

#[derive(Debug, PartialEq)]
struct MethodCallWithReceiver {
    method_call: Ident,
    receiver: Option<Ident>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct Client {
    name: Option<String>,
    sdk: Option<String>,
}

impl MethodVisitor {
    pub(crate) fn new(item: &ItemFn, checks: RequiredPropertiesMap) -> Self {
        let mut visitor = Self {
            clients: analyze_signature(&item.sig),
            method_calls: vec![],
            required_props: checks,
        };
        visitor.visit_item_fn(item);
        visitor
    }

    // TODO some additional tests
    pub(crate) fn find_improper_usages(&self, mut selected_sdks: Vec<String>) -> Vec<UsageFinds> {
        let mut initial: Vec<_> = self.method_calls.iter().rev().collect();
        let mut results: Vec<UsageFinds> = vec![];

        while !initial.is_empty() {
            // go through the method calls until we encounter an SDK function we want to check
            let mut skip_until_relevant_function_call: Vec<_> = initial
                .into_iter()
                .skip_while(|m| !self.required_props.contains_key::<str>(m.method_call.to_string().as_ref()))
                .collect();

            if skip_until_relevant_function_call.is_empty() {
                return results;
            }

            let sdk_function_call = skip_until_relevant_function_call
                .first()
                .expect("just checked that vec is not empty");

            // when we have an SDK function that needs checking, take all the relevant method calls
            // until we encounter a 'send' call or until we encounter an interesting function different to the current one
            let arguments_for_function: Vec<_> = skip_until_relevant_function_call
                .iter()
                .map(|v| v.method_call.to_string())
                .take_while(|v| {
                    v != AWS_SDK_SEND
                        && (*v == sdk_function_call.method_call.to_string() || !self.required_props.contains_key::<str>(v.as_ref()))
                })
                .collect();

            if let Some(receiver) = &sdk_function_call.receiver {
                if !self.clients.is_empty()
                    && !self
                    .clients
                    .iter()
                    .filter_map(|c| c.name.to_owned())
                    .collect::<Vec<String>>()
                    .contains(&&receiver.to_string())
                {
                    // we have clients and none of them match the receiver, meaning this probably isn't a relevant function
                    skip_until_relevant_function_call.drain(0..arguments_for_function.len());
                    initial = skip_until_relevant_function_call;
                    continue;
                }
            }

            let required_props_for_this_method = match self.get_required_props_for(sdk_function_call, &mut selected_sdks) {
                Ok(required) => required,
                Err(sdks) => {
                    // could not find the _right_ props, gather what we already have and break
                    results.push(UsageFinds::Unknown(UnknownUsage {
                        span: sdk_function_call.method_call.span(),
                        method: sdk_function_call.method_call.to_string(),
                        sdks,
                    }));
                    return results;
                }
            };

            // now we can compare our required arguments with the real arguments. if one of the required 'check' values is not present, we have a problem
            let missing_required_args: Vec<_> = required_props_for_this_method
                .1
                .into_iter()
                .map(|c| c.to_string())
                .filter(|c| !arguments_for_function.contains(c))
                .collect();

            if !missing_required_args.is_empty() {
                results.push(UsageFinds::Improper(ImproperUsage {
                    span: sdk_function_call.method_call.span(),
                    method: sdk_function_call.method_call.to_string(),
                    missing: missing_required_args,
                    sdk: required_props_for_this_method.0,
                }));
            }

            // could probably use a find to look for the end of the first relevant results, draining the initial until that index
            skip_until_relevant_function_call.drain(0..arguments_for_function.len());
            initial = skip_until_relevant_function_call;
        }

        results
    }

    /**
     * Algorithm:
     * - if there's only one result (or none at all), return
     * - if there are multiple results, check if they are all the same. If that's true, return any of them
     * - if there are multiple results, and they are not the same, check if the user specified SDKs and return a match. If we still have multiple results, check if the receiver is of any help
     * - if we still the user did not specify an SDK, check the receiver
     * - if we still haven't found a unique match, try using the clients
     * - if everything fails, return an error containing the list of keys (SDKs) that we did find
     */
    fn get_required_props_for<'a>(
        &self,
        function_call: &MethodCallWithReceiver,
        selected_sdks: &mut Vec<String>,
    ) -> Result<(String, Vec<&'a str>), Vec<String>> {
        let hashmaps_with_required_props = self
            .required_props
            .get::<str>(function_call.method_call.to_string().as_ref())
            .expect("should have been verified that the method is present");

        if hashmaps_with_required_props.keys().len() == 1 {
            return Ok((
                hashmaps_with_required_props.keys().next().unwrap().to_string(),
                hashmaps_with_required_props
                    .values()
                    .next()
                    .expect("just checked that there is a key")
                    .to_owned()
            ));
        }
        let (all_results_are_the_same, required_props) = results_that_are_all_the_same(hashmaps_with_required_props);

        if all_results_are_the_same {
            let mut sdks = hashmaps_with_required_props.keys().into_iter().map(|v| v.to_string()).collect::<Vec<_>>();
            sdks.sort_unstable();
            return Ok((
                sdks.join(","),
                required_props,
            ));
        }

        if !selected_sdks.is_empty() {
            let mut results: Vec<(&String, &Vec<&str>)> = selected_sdks
                .iter()
                .filter_map(|sdk| hashmaps_with_required_props.get(&sdk.as_ref()).map(|result| (sdk, result)))
                .collect::<Vec<_>>();

            if results.len() > 1 {
                // receiver can be a tie-breaker
                if let Some(receiver) = &function_call.receiver {
                    let sdk = try_to_get_sdk_from_name(&receiver.to_string());
                    if let Some(found) = results.iter().filter(|r| r.0 == &sdk).collect::<Vec<_>>().pop() {
                        return Ok((sdk, found.1.to_owned()));
                    }
                }
                // at this point we could try to check the client, but those probably won't be of use because if the user selected SDKs X and Y, he probably has clients for both
            }

            if let Some(found) = results.pop() {
                return Ok((
                    selected_sdks.first().expect("called after is_empty check").to_string(),
                    found.1.to_owned()
                ));
            }
        }

        if let Some(receiver) = &function_call.receiver {
            let receiver_as_string = receiver.to_string();
            let receiver_as_client = Client {
                name: Some(receiver_as_string.clone()),
                sdk: None,
            };
            if let Some(found) = self.required_props_for_client(hashmaps_with_required_props, &receiver_as_client) {
                return Ok((
                    try_to_get_sdk_from_name(&receiver_as_client.name.expect("just set the name of the client")),
                    found.to_owned()
                ));
            }
        }

        let mut client_results: Vec<(&Client, Vec<&str>)> = self
            .clients
            .iter()
            .filter_map(|c| {
                self.required_props_for_client(hashmaps_with_required_props, c)
                    .map(|result| (c, result))
            })
            .collect();

        if !client_results.is_empty() {
            if client_results.len() > 1 && function_call.receiver.is_some() {
                // this could hopefully be nicer
                let client_that_matches_receiver_or_default = client_results
                    .iter()
                    .find(|c| {
                        c.0.name.is_some() && c.0.name.as_ref().unwrap().eq(&function_call.receiver.as_ref().unwrap().to_string())
                    })
                    .map(|c| c.clone())
                    .unwrap_or_else(|| client_results.pop().unwrap());
                let client = client_that_matches_receiver_or_default.0;
                let sdk = try_to_get_sdk_from_client(client);

                Ok((sdk, client_that_matches_receiver_or_default.1))
            } else {
                let client_result = client_results.pop().expect("called after is_empty check");
                let client = client_result.0;
                let sdk = try_to_get_sdk_from_client(client);

                Ok((
                    sdk,
                    client_result.1
                ))
            }
        } else {
            Err(hashmaps_with_required_props.keys().map(|key| key.to_string()).collect())
        }
    }

    fn required_props_for_client<'a>(
        &self,
        hashmaps_with_required_props: &HashMap<&'a str, Vec<&'a str>>,
        client: &Client,
    ) -> Option<Vec<&'a str>> {
        match client {
            Client { sdk: Some(sdk), .. } if hashmaps_with_required_props.contains_key(&sdk.to_string().as_ref()) => Some(
                hashmaps_with_required_props
                    .get(&sdk.to_string().as_ref())
                    .expect("just checked that this key is present")
                    .to_owned(),
            ),
            Client { name: Some(name), .. } => {
                let sdk = try_to_get_sdk_from_name(name);

                if hashmaps_with_required_props.contains_key(&sdk.as_ref()) {
                    Some(
                        hashmaps_with_required_props
                            .get(&sdk.as_ref())
                            .expect("just checked that this key is present")
                            .to_owned(),
                    )
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

fn results_that_are_all_the_same<'a>(hashmaps_with_required_props: &HashMap<&str, Vec<&'a str>>) -> (bool, Vec<&'a str>) {
    hashmaps_with_required_props.values().fold((true, vec![]), |acc, curr| {
        if acc.1.is_empty() || !acc.0 {
            (acc.0, curr.to_owned())
        } else if acc.1 == *curr {
            (true, curr.to_owned())
        } else {
            (false, curr.to_owned())
        }
    })
}

fn try_to_get_sdk_from_client(client: &Client) -> String {
    if let Some(sdk) = client.sdk.as_ref() {
        sdk.to_string()
    } else if let Some(name) = client.name.as_ref() {
        try_to_get_sdk_from_name(name)
    } else {
        "unknown".to_string()
    }
}

fn try_to_get_sdk_from_name(name: &String) -> String {
    name.replace("client", "").replace('_', "")
}

impl<'ast> Visit<'ast> for MethodVisitor {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let method_call = node.method.clone();

        match node.receiver.as_ref() {
            Expr::Path(p) => {
                // not another method call, so with path we've come to the end of the chain, and found who is calling the method(s)
                let segments = p.path.segments.clone();
                // presumably, there could be multiple segments. but this will be OK most of the time
                let receiver = segments.into_iter().map(|s| s.ident).collect::<Vec<Ident>>().pop();

                self.method_calls.push(MethodCallWithReceiver { method_call, receiver });
            }
            Expr::Field(f) => {
                // call on a field, e.g. object.client or self.client
                match &f.member {
                    Member::Named(field_name) => {
                        let receiver = Some(field_name.clone());
                        self.method_calls.push(MethodCallWithReceiver { method_call, receiver });
                    }
                    Member::Unnamed(_) => {
                        // unnamed is useless when it comes to determining the receiver
                        self.method_calls.push(MethodCallWithReceiver {
                            method_call,
                            receiver: None,
                        })
                    }
                }
            }
            _ => self.method_calls.push(MethodCallWithReceiver {
                method_call,
                receiver: None,
            }),
        }

        visit::visit_expr_method_call(self, node);
    }

    fn visit_local(&mut self, node: &'ast Local) {
        if let Some(init) = &node.init {
            match init.expr.as_ref() {
                Expr::Call(call) => {
                    match call.func.as_ref() {
                        Expr::Path(path) => {
                            let segments: Vec<String> = path.path.segments.iter().map(|seg| seg.ident.to_string()).collect();

                            if segments.contains(&"Client".to_string()) {
                                // this might be an AWS client, retrieve the name and look for the SDK
                                let aws_sdk = segments
                                    .iter()
                                    .find(|s| s.contains(AWS_SDK_PREFIX))
                                    .map(|s| s.replace(AWS_SDK_PREFIX, "").to_string());
                                let name = match &node.pat {
                                    Pat::Ident(i) => Some(i.ident.to_string()),
                                    _ => None,
                                };

                                self.clients.insert(Client { name, sdk: aws_sdk });
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        visit::visit_local(self, node);
    }
}

fn analyze_signature(sig: &Signature) -> HashSet<Client> {
    sig.inputs
        .iter()
        .filter_map(|i| {
            match i {
                FnArg::Typed(ty) => {
                    match ty.ty.as_ref() {
                        Type::Path(p) => {
                            let mut segments_as_strings: Vec<String> = p.path.segments.iter().map(|s| s.ident.to_string()).collect();

                            if !segments_as_strings.is_empty() {
                                let last = segments_as_strings.pop().expect("at least one element");

                                if last == "Client" {
                                    // this might be an AWS client, retrieve the name and path if any
                                    let aws_sdk = segments_as_strings
                                        .pop()
                                        .filter(|earlier_segment| earlier_segment.starts_with(AWS_SDK_PREFIX))
                                        .map(|v| v.replace(AWS_SDK_PREFIX, ""));

                                    let client_name = match ty.pat.as_ref() {
                                        Pat::Ident(i) => Some(i.ident.to_string()),
                                        _ => None,
                                    };

                                    return Some(Client {
                                        name: client_name,
                                        sdk: aws_sdk,
                                    });
                                }
                            }
                            None
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod test {
    use core::default::Default;
    use std::collections::{HashMap, HashSet};

    use proc_macro2::{Ident, Span};
    use quote::quote;
    use syn::Expr::MethodCall;
    use syn::Stmt;
    use syn::visit::Visit;

    use crate::visitor::{analyze_signature, Client, ImproperUsage, MethodCallWithReceiver, MethodVisitor, UsageFinds};

    #[test]
    fn visit_expr_method_call_relevant_aws_sdk_call() {
        let statement: Stmt = syn::parse2(quote!(sqs_client.receive_message().queue_url(queue_url).send();)).unwrap();
        let mut visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![],
            required_props: Default::default(),
        };

        match statement {
            Stmt::Expr(MethodCall(method_call), _) => visitor.visit_expr_method_call(&method_call),
            _ => unreachable!("the above creates and parses an expression method call"),
        }

        assert_eq!(
            visitor.method_calls,
            vec![
                MethodCallWithReceiver {
                    method_call: Ident::new("send", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("queue_url", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("receive_message", Span::call_site()),
                    receiver: Some(Ident::new("sqs_client", Span::call_site())),
                },
            ]
        );
    }

    #[test]
    fn visit_expr_method_call_other_method_call() {
        let statement: Stmt = syn::parse2(quote!(some_thing.to_string();)).unwrap();
        let mut visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![],
            required_props: Default::default(),
        };

        match statement {
            Stmt::Expr(MethodCall(method_call), _) => visitor.visit_expr_method_call(&method_call),
            _ => unreachable!("the above creates and parses an expression method call"),
        }

        assert_eq!(
            visitor.method_calls,
            vec![MethodCallWithReceiver {
                method_call: Ident::new("to_string", Span::call_site()),
                receiver: Some(Ident::new("some_thing", Span::call_site())),
            }, ]
        );
    }

    #[test]
    fn visit_expr_method_call_method_call_with_self() {
        let statement: Stmt = syn::parse2(quote!(self.sqs_client.receive_message().queue_url(queue_url).send();)).unwrap();
        let mut visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![],
            required_props: Default::default(),
        };

        match statement {
            Stmt::Expr(MethodCall(method_call), _) => visitor.visit_expr_method_call(&method_call),
            _ => unreachable!("the above creates and parses an expression method call"),
        }

        assert_eq!(
            visitor.method_calls,
            vec![
                MethodCallWithReceiver {
                    method_call: Ident::new("send", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("queue_url", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("receive_message", Span::call_site()),
                    receiver: Some(Ident::new("sqs_client", Span::call_site())),
                },
            ]
        );
    }

    #[test]
    fn visit_local_init_full_client() {
        let statement: Stmt = syn::parse2(quote!(let a_client = aws_sdk_sqs::Client::new();)).unwrap();
        let mut visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![],
            required_props: Default::default(),
        };

        match statement {
            Stmt::Local(local) => visitor.visit_local(&local),
            _ => unreachable!("the above creates and parses a local init"),
        };

        assert_eq!(
            visitor.clients,
            HashSet::from([Client {
                name: Some("a_client".to_string()),
                sdk: Some("sqs".to_string()),
            }])
        );
    }

    #[test]
    fn visit_local_init_simple_client() {
        let statement: Stmt = syn::parse2(quote!(let simple_client = Client::new();)).unwrap();
        let mut visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![],
            required_props: Default::default(),
        };

        match statement {
            Stmt::Local(local) => visitor.visit_local(&local),
            _ => unreachable!("the above creates and parses a local init"),
        };

        assert_eq!(
            visitor.clients,
            HashSet::from([Client {
                name: Some("simple_client".to_string()),
                sdk: None,
            }])
        );
    }

    #[test]
    fn analyze_local_init_no_client() {
        let statement: Stmt = syn::parse2(quote!(let simple_client = vec![];)).unwrap();
        let mut visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![],
            required_props: Default::default(),
        };

        match statement {
            Stmt::Local(local) => visitor.visit_local(&local),
            _ => unreachable!("the above creates and parses a local init"),
        };

        assert!(visitor.clients.is_empty());
    }

    #[test]
    fn analyze_signature_full_aws_client() {
        let sig = syn::parse2(quote!(fn full(a_client: aws_sdk_s3::Client))).unwrap();

        let actual = analyze_signature(&sig);

        assert_eq!(
            actual,
            HashSet::from([Client {
                name: Some("a_client".to_string()),
                sdk: Some("s3".to_string()),
            }])
        );
    }

    #[test]
    fn analyze_signature_full_aws_client_with_other_args_and_return_value() {
        let sig = syn::parse2(quote!(fn full(something: &str, a_client: aws_sdk_s3::Client, another_arg: u32) -> String)).unwrap();

        let actual = analyze_signature(&sig);

        assert_eq!(
            actual,
            HashSet::from([Client {
                name: Some("a_client".to_string()),
                sdk: Some("s3".to_string()),
            }])
        );
    }

    #[test]
    fn analyze_signature_simple_client_with_other_args() {
        let sig = syn::parse2(quote!(fn simp(something: &str, simple_client: Client))).unwrap();

        let actual = analyze_signature(&sig);

        assert_eq!(
            actual,
            HashSet::from([Client {
                name: Some("simple_client".to_string()),
                sdk: None,
            }])
        );
    }

    #[test]
    fn analyze_signature_no_args_so_no_client() {
        let sig = syn::parse2(quote!(fn no_args() -> String)).unwrap();

        let actual = analyze_signature(&sig);

        assert!(actual.is_empty());
    }

    #[test]
    fn analyze_signature_other_args_no_client() {
        let sig = syn::parse2(quote!(fn other_args(something: String) -> String)).unwrap();

        let actual = analyze_signature(&sig);

        assert!(actual.is_empty());
    }

    #[test]
    fn get_required_props_for_only_one_match() {
        let mut required_props = HashMap::new();
        required_props.insert("some_call", HashMap::from([("s3", vec!["required_call"])]));
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![],
            required_props,
        };
        let call = MethodCallWithReceiver {
            method_call: Ident::new("some_call", Span::call_site()),
            receiver: None,
        };

        let actual = visitor.get_required_props_for(&call, &mut vec![]).unwrap();

        assert_eq!(actual, ("s3".to_string(), vec!["required_call"]));
    }

    #[test]
    fn get_required_props_for_two_identical_matches_pick_one() {
        let mut required_props = HashMap::new();
        required_props.insert(
            "some_call",
            HashMap::from([("s3", vec!["required_call"]), ("sqs", vec!["required_call"])]),
        );
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![],
            required_props,
        };
        let call = MethodCallWithReceiver {
            method_call: Ident::new("some_call", Span::call_site()),
            receiver: None,
        };

        let actual = visitor.get_required_props_for(&call, &mut vec![]).unwrap();

        assert_eq!(actual, ("s3,sqs".to_string(), vec!["required_call"]));
    }

    #[test]
    fn get_required_props_for_two_different_matches_pick_correct_sdk() {
        let mut required_props = HashMap::new();
        required_props.insert(
            "some_call",
            HashMap::from([("s3", vec!["required_call"]), ("sqs", vec!["different_call"])]),
        );
        let visitor = MethodVisitor {
            clients: HashSet::from([Client {
                name: None,
                sdk: Some("sqs".to_string()),
            }]),
            method_calls: vec![],
            required_props,
        };
        let call = MethodCallWithReceiver {
            method_call: Ident::new("some_call", Span::call_site()),
            receiver: None,
        };

        let actual = visitor.get_required_props_for(&call, &mut vec![]).unwrap();

        assert_eq!(actual, ("sqs".to_string(), vec!["different_call"]));
    }

    #[test]
    fn get_required_props_for_two_different_matches_pick_correct_client_prefix() {
        let mut required_props = HashMap::new();
        required_props.insert(
            "some_call",
            HashMap::from([("s3", vec!["required_call"]), ("sqs", vec!["different_call"])]),
        );
        let visitor = MethodVisitor {
            clients: HashSet::from([Client {
                name: Some("sqs_client".to_string()),
                sdk: None,
            }]),
            method_calls: vec![],
            required_props,
        };
        let call = MethodCallWithReceiver {
            method_call: Ident::new("some_call", Span::call_site()),
            receiver: None,
        };

        let actual = visitor.get_required_props_for(&call, &mut vec![]).unwrap();

        assert_eq!(actual, ("sqs".to_string(), vec!["different_call"]));
    }

    #[test]
    fn get_required_props_for_two_different_matches_pick_correct_client() {
        let mut required_props = HashMap::new();
        required_props.insert(
            "some_call",
            HashMap::from([("s3", vec!["required_call"]), ("sqs", vec!["different_call"])]),
        );
        let visitor = MethodVisitor {
            clients: HashSet::from([Client {
                name: Some("sqs".to_string()),
                sdk: None,
            }]),
            method_calls: vec![],
            required_props,
        };
        let call = MethodCallWithReceiver {
            method_call: Ident::new("some_call", Span::call_site()),
            receiver: None,
        };

        let actual = visitor.get_required_props_for(&call, &mut vec![]).unwrap();

        assert_eq!(actual, ("sqs".to_string(), vec!["different_call"]));
    }

    #[test]
    fn find_improper_usages_no_method_calls_or_checks_return_zero_usages() {
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![],
            required_props: Default::default(),
        };

        let improper = visitor.find_improper_usages(vec![]);

        assert_eq!(improper.len(), 0);
    }

    #[test]
    fn find_improper_usages_method_calls_but_no_checks_return_zero_usages() {
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![MethodCallWithReceiver {
                method_call: Ident::new("some_call", Span::call_site()),
                receiver: None,
            }],
            required_props: Default::default(),
        };

        let improper = visitor.find_improper_usages(vec![]);

        assert_eq!(improper.len(), 0);
    }

    #[test]
    fn find_improper_usages_method_calls_but_no_matching_checks_return_zero_usages() {
        let mut required_props = HashMap::new();
        required_props.insert("some_other_call", HashMap::from([("s3", vec!["required_call"])]));
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![MethodCallWithReceiver {
                method_call: Ident::new("some_call", Span::call_site()),
                receiver: None,
            }],
            required_props,
        };

        let improper = visitor.find_improper_usages(vec![]);

        assert_eq!(improper.len(), 0);
    }

    #[test]
    fn find_improper_usages_method_calls_not_ending_with_send_and_unknown_return_single_match() {
        let mut required_props = HashMap::new();
        required_props.insert(
            "send_message",
            HashMap::from([("s3", vec!["required_call", "required_call_that_is_missing"])]),
        );
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![
                MethodCallWithReceiver {
                    method_call: Ident::new("unknown", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("required_call", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send_message", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("other_unknown", Span::call_site()),
                    receiver: None,
                },
            ],
            required_props,
        };

        let improper = visitor.find_improper_usages(vec![]);

        assert_eq!(improper.len(), 1);
        let improper = get_improper_usages(improper);
        let first = improper.first().unwrap();
        assert_eq!(first.method, "send_message");
        assert_eq!(first.missing, vec!["required_call_that_is_missing"]);
    }

    #[test]
    fn find_improper_usages_method_calls_ending_with_send_and_unknown_return_single_match() {
        let mut required_props = HashMap::new();
        required_props.insert(
            "send_message",
            HashMap::from([("s3", vec!["required_call", "required_call_that_is_missing"])]),
        );
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![
                MethodCallWithReceiver {
                    method_call: Ident::new("unknown", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("required_call", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send_message", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("other_unknown", Span::call_site()),
                    receiver: None,
                },
            ],
            required_props,
        };

        let improper = visitor.find_improper_usages(vec![]);

        assert_eq!(improper.len(), 1);
        let improper = get_improper_usages(improper);
        let first = improper.first().unwrap();
        assert_eq!(first.method, "send_message");
        assert_eq!(first.missing, vec!["required_call_that_is_missing"]);
    }

    #[test]
    fn find_improper_usages_method_calls_ending_with_send_and_unknown_return_multiple_matches() {
        let mut required_props = HashMap::new();
        required_props.insert(
            "send_message",
            HashMap::from([("s3", vec!["required_call", "second_required_call"])]),
        );
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![
                MethodCallWithReceiver {
                    method_call: Ident::new("unknown", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("something_optional", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send_message", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("other_unknown", Span::call_site()),
                    receiver: None,
                },
            ],
            required_props,
        };

        let improper = visitor.find_improper_usages(vec![]);

        assert_eq!(improper.len(), 1);
        let mut improper = get_improper_usages(improper);
        let first = improper.pop().unwrap();
        assert_eq!(first.method, "send_message");
        assert_eq!(first.missing, vec!["required_call", "second_required_call"]);
    }

    #[test]
    fn find_improper_usages_multiple_methods_each_with_missing() {
        let mut required_props = HashMap::new();
        required_props.insert(
            "send_message",
            HashMap::from([("s3", vec!["required_send_call", "required_send_call_that_is_missing"])]),
        );
        required_props.insert("receive_message", HashMap::from([("s3", vec!["required_receive_call"])]));
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![
                MethodCallWithReceiver {
                    method_call: Ident::new("send", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("optional_stuff", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("receive_message", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("unknown", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("required_send_call", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send_message", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("other_unknown", Span::call_site()),
                    receiver: None,
                },
            ],
            required_props,
        };

        let improper = visitor.find_improper_usages(vec![]);

        assert_eq!(improper.len(), 2);
        let mut improper = get_improper_usages(improper);
        let second = improper.pop().unwrap();
        let first = improper.pop().unwrap();
        assert_eq!(first.method, "send_message");
        assert_eq!(first.missing, vec!["required_send_call_that_is_missing"]);
        assert_eq!(second.method, "receive_message");
        assert_eq!(second.missing, vec!["required_receive_call"]);
    }

    #[test]
    fn find_improper_usages_multiple_methods_one_with_multiple_missing_one_with_single() {
        let mut required_props = HashMap::new();
        required_props.insert(
            "send_message",
            HashMap::from([("s3", vec!["required_send_call", "required_send_call_that_is_missing"])]),
        );
        required_props.insert("receive_message", HashMap::from([("s3", vec!["required_receive_call"])]));
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![
                MethodCallWithReceiver {
                    method_call: Ident::new("send", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("optional_stuff", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("receive_message", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("unknown", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("something_something", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send_message", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("other_unknown", Span::call_site()),
                    receiver: None,
                },
            ],
            required_props,
        };

        let improper = visitor.find_improper_usages(vec![]);

        assert_eq!(improper.len(), 2);
        let mut improper = get_improper_usages(improper);
        let second = improper.pop().unwrap();
        let first = improper.pop().unwrap();
        assert_eq!(first.method, "send_message");
        assert_eq!(first.missing, vec!["required_send_call", "required_send_call_that_is_missing"]);
        assert_eq!(second.method, "receive_message");
        assert_eq!(second.missing, vec!["required_receive_call"]);
    }

    #[test]
    fn find_improper_usages_multiple_methods_everything_ok() {
        let mut required_props = HashMap::new();
        required_props.insert(
            "send_message",
            HashMap::from([("s3", vec!["required_send_call", "required_send_call_that_is_missing"])]),
        );
        required_props.insert("receive_message", HashMap::from([("s3", vec!["required_receive_call"])]));
        let visitor = MethodVisitor {
            clients: HashSet::new(),
            method_calls: vec![
                MethodCallWithReceiver {
                    method_call: Ident::new("send", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("required_receive_call", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("receive_message", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("unknown", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("something_something", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("required_send_call", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("required_send_call_that_is_missing", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("send_message", Span::call_site()),
                    receiver: None,
                },
                MethodCallWithReceiver {
                    method_call: Ident::new("other_unknown", Span::call_site()),
                    receiver: None,
                },
            ],
            required_props,
        };

        let improper = visitor.find_improper_usages(vec![]);

        assert_eq!(improper.len(), 0);
    }

    fn get_improper_usages(finds: Vec<UsageFinds>) -> Vec<ImproperUsage> {
        finds.into_iter().fold(vec![], |mut acc, curr| match curr {
            UsageFinds::Improper(i) => {
                acc.push(i);
                acc
            }
            UsageFinds::Unknown(_) => panic!("Found an unknown while only expecting improper findings in vec"),
        })
    }
}
