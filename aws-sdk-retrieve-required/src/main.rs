use anyhow::{Context, Result};
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use reqwest::blocking::Client;
use scraper::selectable::Selectable;
use scraper::{Html, Selector};
use serde::Serialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Error};

const AWS_SDK_VERSION: &str = "latest"; // alternatively, pick a concrete version, i.e. "1.18.0"

#[derive(Serialize)]
struct Record<'a> {
    service: &'a str,
    method_name: String,
    property_names: String,
}

fn main() -> Result<()> {
    // setup
    let services = retrieve_services_from_file()?;
    let client = Client::new();
    let class_selector = Selector::parse(".impl-items > details").unwrap();
    // `section h4` because there are sometimes one or more code tags between the two
    let method_selector = Selector::parse("summary > section h4 > a").unwrap();
    let properties_selector = Selector::parse("div > ul > li > ul > li").unwrap();
    let property_selector = Selector::parse("code").unwrap();

    // retrieve props per service
    let results = services
        .par_iter()
        .filter(|s| !s.is_empty())
        .map(|service| {
            println!("Retrieving {}", service);
            let docs = retrieve_aws_docs(&client, service)?;
            let required_props_per_method = analyze_text(
                &class_selector,
                &method_selector,
                &properties_selector,
                &property_selector,
                &docs,
                service,
            )?;
            write_to_file(service, required_props_per_method)?;
            Ok(())
        })
        .collect::<Vec<Result<()>>>();

    for result in results {
        match result {
            Ok(_) => {}
            Err(e) => {
                println!("An error occurred: {}", e);
            }
        }
    }

    Ok(())
}

fn retrieve_services_from_file() -> Result<Vec<String>, Error> {
    let file = File::open("./input/names_of_sdk_crates")?; // these names are based on the directories of the aws_rust_sdk on GitHub
    let reader = BufReader::new(file);
    let services: Vec<String> = reader.lines().collect::<Result<Vec<_>, Error>>()?;
    Ok(services)
}

fn retrieve_aws_docs(client: &Client, service: &str) -> Result<String> {
    let url = format!(
        "https://docs.rs/aws-sdk-{}/{}/aws_sdk_{}/client/struct.Client.html",
        service, AWS_SDK_VERSION, service
    );
    let result = client
        .get(&url)
        .send()
        .with_context(|| format!("call to url {} for {} failed", &url, service))?;
    result
        .text()
        .with_context(|| format!("call to get text for url {} for {} failed", &url, service))
}

fn sanitize_property(prop_name: String) -> String {
    prop_name
        .split('(')
        .next()
        .expect("Split to always have at least one element")
        .to_string()
}

fn analyze_text<'a>(
    class_selector: &Selector,
    method_selector: &Selector,
    properties_selector: &Selector,
    property_selector: &Selector,
    docs: &str,
    service: &'a str,
) -> Result<Vec<Record<'a>>> {
    let document = Html::parse_document(docs);

    let mut required_props_per_method = vec![];

    for element in document.select(class_selector) {
        let method_name = element
            .select(method_selector)
            .next()
            .with_context(|| format!("failed to find method name for {}", service))?
            .inner_html();
        let mut property_names = vec![];

        for property in element.select(properties_selector) {
            if property.inner_html().contains("required: <strong>true</strong>") {
                // the other <code> contents are things like set_queue_url, which might also be useful, but ignoring for now
                let property_name = property
                    .select(property_selector)
                    .next()
                    .with_context(|| format!("failed to find the property name for {}", service))?;
                property_names.push(sanitize_property(property_name.inner_html()));
            }
        }

        if !property_names.is_empty() {
            required_props_per_method.push(Record {
                service,
                method_name,
                property_names: property_names.join(" "),
            })
        }
    }

    Ok(required_props_per_method)
}

fn write_to_file(service: &str, required_props_per_method: Vec<Record>) -> Result<()> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_path(format!("output/{}.csv", service))
        .with_context(|| format!("failed to created writer for {}", &service))?;

    for el in required_props_per_method {
        writer
            .serialize(el)
            .with_context(|| format!("failed to write record for {}", &service))?;
    }

    Ok(())
}
