#!/usr/bin/env python3

""" a file procuding benchmarks and saving counterexample outputs 
for thesis evaluation
fully based on x.py and using its benchmarking code'
"""
import sys
if sys.version_info[0] < 3:
    print('You need to run this script with Python 3.')
    sys.exit(1)

import os
import platform
import subprocess
import glob
import csv
import time
import json 
import signal
import pathlib

def run_counterexamples(path, target):
    # runs prusti with counterexamples enabled on all files in the
    # specified folder
    curr_wd = os.getcwd()

    os.environ["PRUSTI_COUNTEREXAMPLE"]="true"
    prusti_rustc_exe = get_prusti_rustc_path_for_benchmark()

    file_set = set()

    for dir_, _, files in os.walk(path):
        for file_name in files:
            rel_dir = os.path.relpath(dir_, path)
            rel_file = os.path.join(rel_dir, file_name)
            file = os.path.join(path, rel_file)
            if file_name.endswith(".rs"):
                target_path = target + file_name + ".txt"
                command = prusti_rustc_exe + " --edition=2018 " + file + " > " + target_path + " 2>" +target_path
                print(command)
                os.system(command)

def run_benchmarks(args):
    """Run the benchmarks and print the time in a json file"""
    warmup_iterations = 6
    bench_iterations = 10
    warmup_path = "prusti-tests/tests/verify/pass/quick/fibonacci.rs"
    prusti_server_exe = get_prusti_server_path_for_benchmark()
    server_port = "12345"
    output_dir = "benchmark-output"
    benchmark_csv = "counterexample-thesis-resources/benchmarked-files-counterexample.csv"
    results = {}
    
    print_name_suffix = ("-" + args[0]) if len(args) > 0 else ''

    env = get_env()
    print("Starting prusti-server ({})", prusti_server_exe)
    server_process = subprocess.Popen([prusti_server_exe,"--port",server_port], env=env)
    time.sleep(2)
    if server_process.poll() != None:
        raise RuntimeError('Could not start prusti-server') 

    env["PRUSTI_SERVER_ADDRESS"]="localhost:" + server_port
    try:
        print("Starting warmup of the server")
        for i in range(warmup_iterations):
            t = measure_prusti_time(warmup_path, env)
            print("warmup run {} took {}", i + 1, t)
        
        print("Finished warmup. Starting benchmark")
        with open(benchmark_csv) as csv_file:
            csv_reader = csv.reader(csv_file, delimiter=',')
            for row in csv_reader:
                file_path = row[0]
                results[file_path] = []
                print("Starting to benchmark {}", file_path)
                for i in range(bench_iterations):
                    t = measure_prusti_time(file_path, env)
                    results[file_path].append(t)
    finally:
        print("terminating prusti-server")
        server_process.send_signal(signal.SIGINT)

    if not os.path.exists(output_dir):
        os.makedirs(output_dir)

    json_result = json.dumps(results, indent = 2)
    timestamp = time.time()
    output_file = os.path.join(output_dir, "benchmark" + print_name_suffix + str(timestamp) + ".json")
    with open(output_file, "w") as outfile: 
        outfile.write(json_result) 
    
    print("Wrote results of benchmark to {}", output_file)



def run_command(args, env=None):
    """Run a command with the given arguments."""
    if env is None:
        env = get_env()
    completed = subprocess.run(args, env=env)


def error(template, *args, **kwargs):
    """Print the error and exit the program."""
    print(template.format(*args, **kwargs))
    sys.exit(1)

def shell(command, term_on_nzec=True):
    """Run a shell command."""
    print("Running a shell command: ", command)
    if not dry_run:
        completed = subprocess.run(command.split())
        if completed.returncode != 0 and term_on_nzec:
            sys.exit(completed.returncode)
        return completed.returncode


def get_prusti_server_path_for_benchmark():
    project_root_dir = os.path.dirname(os.path.realpath(sys.argv[0]))
    
    if sys.platform in ("linux", "linux2"):
        return os.path.join(project_root_dir, 'target', 'release', 'prusti-server-driver')
    else:
        error("unsupported platform for benchmarks: {}", sys.platform)


def get_prusti_rustc_path_for_benchmark():
    project_root_dir = os.path.dirname(os.path.realpath(sys.argv[0]))
    
    if sys.platform in ("linux", "linux2"):
        return os.path.join(project_root_dir, 'target', 'release', 'prusti-rustc')
    else:
        error("unsupported platform for benchmarks: {}", sys.platform)

def set_env_variables(env, variables):
    """Set the given environment variables in `env` if not already set, merging special variables."""
    for name, value in variables:
        if name not in env:
            env[name] = value
        elif name in ("PATH", "LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"):
            if sys.platform == "win32":
                env[name] += ";" + value
            else:
                env[name] += ":" + value
        print("env: {}={}", name, env[name])

def get_linux_env():
    """Get environment variables for Linux."""
    java_home = get_var_or('JAVA_HOME', default_linux_java_loc())
    variables = [
        ('JAVA_HOME', java_home),
        ('RUST_TEST_THREADS', '1'),
    ]
    if os.path.exists(java_home):
        ld_library_path = None
        for root, _, files in os.walk(java_home):
            if 'libjvm.so' in files:
                ld_library_path = root
                break
        if ld_library_path is None:
            print("could not find libjvm.so in {}", java_home)
        else:
            variables.append(('LD_LIBRARY_PATH', ld_library_path))
    viper_home = get_var_or('VIPER_HOME', os.path.abspath('viper_tools/backends'))
    if os.path.exists(viper_home):
        variables.append(('VIPER_HOME', viper_home))
    z3_exe = os.path.abspath(os.path.join(viper_home, '../z3/bin/z3'))
    if os.path.exists(z3_exe):
        variables.append(('Z3_EXE', z3_exe))
    return variables

def default_linux_java_loc():
    if os.path.exists('/usr/lib/jvm/default-java'):
        return '/usr/lib/jvm/default-java'
    elif os.path.exists('/usr/lib/jvm/default'):
        return '/usr/lib/jvm/default'
    print("Could not determine default java location.")


def get_var_or(name, default):
    """If environment variable `name` set, return its value or `default`."""
    if name in os.environ:
        return os.environ[name]
    else:
        return default


def get_env():
    """Returns the environment with the variables set."""
    env = os.environ.copy()
    if sys.platform in ("linux", "linux2"):
        # Linux
        set_env_variables(env, get_linux_env())
    elif sys.platform == "darwin":
        # Mac
        set_env_variables(env, get_mac_env())
    elif sys.platform == "win32":
        # Windows
        set_env_variables(env, get_win_env())
    else:
        error("unsupported platform: {}", sys.platform)
    return env


def measure_prusti_time(input_path, env):
    prusti_rustc_exe = get_prusti_rustc_path_for_benchmark()
    start_time = time.perf_counter()
    run_command([prusti_rustc_exe,"--edition=2018", input_path], env=env)
    end_time = time.perf_counter()
    elapsed = end_time - start_time
    return elapsed  

def main(argv):
    for i, arg in enumerate(argv):
        if arg == 'run-benchmarks':
            run_benchmarks(argv[i+1:])
            break
        elif arg == 'get-counterexample-outputs':
            run_counterexamples(argv[i+1], argv[i+2]) 
            break
        else:
            print("not a valid flag: ", arg)


if __name__ == '__main__':
    main(sys.argv[1:])
