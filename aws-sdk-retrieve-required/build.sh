#!/bin/bash

set -euo pipefail

rm -r output
mkdir output

cargo run
cd output && cat *.csv >> ../required_props_info.csv && cd ..
mv required_props_info.csv ../aws-sdk-compile-checks-macro/required_properties_info/required_props_info.csv
