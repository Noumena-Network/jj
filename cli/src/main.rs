// Copyright 2020 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use jj_cli::cli_util::CliRunner;
use jj_cli::jjapi_backend;

fn main() -> std::process::ExitCode {
    let mut runner = CliRunner::init().version(env!("JJ_VERSION"));
    runner = runner.add_store_factories(jjapi_backend::jjapi_store_factories());
    let mut working_copy_factories = jj_lib::workspace::WorkingCopyFactories::new();
    jj_lib::workspace::register_edenfs_working_copy_factory(&mut working_copy_factories);
    runner = runner.add_working_copy_factories(working_copy_factories);
    runner.run().into()
}
