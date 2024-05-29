# AWS SDK Compile Checks

This repository consists of three projects:
- aws-sdk-retrieved-required: has code for retrieving a list of required properties for calls made with the AWS SDK
- aws-sdk-compile-checks-macro: has a macro (`required_props`) that checks for the presence of those required properties in (suspected) AWS SDK calls
- aws-sdk-compile-checks-usage: has usage examples and black box tests for the macro
