#!/usr/bin/env python3

"""A wrapper for cargo that sets up the Prusti environment."""

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
import shutil

verbose = False
dry_run = False


def default_linux_java_loc():
    if os.path.exists('/usr/lib/jvm/default-java'):
        return '/usr/lib/jvm/default-java'
    elif os.path.exists('/usr/lib/jvm/default'):
        return '/usr/lib/jvm/default'
    report("Could not determine default java location.")


def report(template, *args, **kwargs):
    """Print the message if `verbose` is `True`."""
    if verbose:
        print(template.format(*args, **kwargs))


def error(template, *args, **kwargs):
    """Print the error and exit the program."""
    print(template.format(*args, **kwargs))
    sys.exit(1)


def get_var_or(name, default):
    """If environment variable `name` set, return its value or `default`."""
    if name in os.environ:
        return os.environ[name]
    else:
        return default


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
            report("could not find libjvm.so in {}", java_home)
        else:
            variables.append(('LD_LIBRARY_PATH', ld_library_path))
    viper_home = get_var_or('VIPER_HOME', os.path.abspath('viper_tools/server'))
    if os.path.exists(viper_home):
        variables.append(('VIPER_HOME', viper_home))
    z3_exe = os.path.abspath(os.path.join(viper_home, '../z3/bin/z3'))
    if os.path.exists(z3_exe):
        variables.append(('Z3_EXE', z3_exe))
    return variables


def get_mac_env():
    """Get environment variables for Mac."""
    java_home = get_var_or('JAVA_HOME', None)
    if java_home is None:
        java_home = subprocess.run(["/usr/libexec/java_home"], stdout=subprocess.PIPE, encoding="utf8").stdout.strip()
    variables = [
        ('JAVA_HOME', java_home),
        ('RUST_TEST_THREADS', '1'),
    ]
    if os.path.exists(java_home):
        ld_library_path = None
        for root, _, files in os.walk(java_home):
            if 'libjli.dylib' in files:
                ld_library_path = root
                break
        if ld_library_path is None:
            report("could not find libjli.dylib in {}", java_home)
        else:
            variables.append(('LD_LIBRARY_PATH', ld_library_path))
            variables.append(('DYLD_LIBRARY_PATH', ld_library_path))
    viper_home = get_var_or('VIPER_HOME', os.path.abspath('viper_tools/server'))
    if os.path.exists(viper_home):
        variables.append(('VIPER_HOME', viper_home))
    z3_exe = os.path.abspath(os.path.join(viper_home, '../z3/bin/z3'))
    if os.path.exists(z3_exe):
        variables.append(('Z3_EXE', z3_exe))
    return variables


def get_win_env():
    """Get environment variables for Windows."""
    java_home = get_var_or('JAVA_HOME', None)
    variables = [
        ('JAVA_HOME', java_home),
        ('RUST_TEST_THREADS', '1'),
    ]
    if os.path.exists(java_home):
        library_path = None
        for root, _, files in os.walk(java_home):
            if 'jvm.dll' in files:
                library_path = root
                break
        if library_path is None:
            report("could not find jvm.dll in {}", java_home)
        else:
            variables.append(('PATH', library_path))
    viper_home = get_var_or('VIPER_HOME', os.path.abspath(os.path.join('viper_tools', 'server')))
    if os.path.exists(viper_home):
        variables.append(('VIPER_HOME', viper_home))
    else:
        report("could not find VIPER_HOME in {}", viper_home)
    z3_exe = os.path.abspath(os.path.join(viper_home, os.path.join('..', 'z3', 'bin', 'z3.exe')))
    if os.path.exists(z3_exe):
        variables.append(('Z3_EXE', z3_exe))
    return variables


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
        report("env: {}={}", name, env[name])


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


def run_command(args, env=None):
    """Run a command with the given arguments."""
    if env is None:
        env = get_env()
    completed = subprocess.run(args, env=env)
    if completed.returncode != 0:
        sys.exit(completed.returncode)


def shell(command, term_on_nzec=True):
    """Run a shell command."""
    print("Running a shell command: ", command)
    if not dry_run:
        completed = subprocess.run(command.split())
        if completed.returncode != 0 and term_on_nzec:
            sys.exit(completed.returncode)
        return completed.returncode


def cargo(args):
    """Run cargo with the given arguments."""
    run_command(['cargo'] + args)


def viper_version():
    with open("viper-toolchain", "r") as file:
        return file.read().strip()


def setup_ubuntu():
    """Install the dependencies on Ubuntu."""
    # Install dependencies.
    shell('sudo apt-get update')
    shell('sudo apt-get install -y '
          'build-essential pkg-config '
          'curl gcc libssl-dev')
    # Download Viper.
    shell(
        'curl https://github.com/viperproject/viper-ide/releases/'
        'download/{}/ViperToolsLinux.zip -Lo ViperToolsLinux.zip'.format(viper_version())
    )
    if os.path.exists('viper_tools'):
        shutil.rmtree('viper_tools')
    shell('unzip ViperToolsLinux.zip -d viper_tools')
    os.remove('ViperToolsLinux.zip')


def setup_linux():
    """Install the dependencies on generic Linux."""
    shell(
        'curl https://github.com/viperproject/viper-ide/releases/'
        'download/{}/ViperToolsLinux.zip -Lo ViperToolsLinux.zip'.format(viper_version())
    )
    if os.path.exists('viper_tools'):
        shutil.rmtree('viper_tools')
    shell('unzip ViperToolsLinux.zip -d viper_tools')
    os.remove('ViperToolsLinux.zip')


def setup_mac():
    """Install the dependencies on Mac."""
    # Non-Viper dependencies must be installed manually.
    # Download Viper.
    shell(
        'curl https://github.com/viperproject/viper-ide/releases/'
        'download/{}/ViperToolsMac.zip -Lo ViperToolsMac.zip'.format(viper_version())
    )
    if os.path.exists('viper_tools'):
        shutil.rmtree('viper_tools')
    shell('unzip ViperToolsMac.zip -d viper_tools')
    os.remove('ViperToolsMac.zip')


def setup_win():
    """Install the dependencies on Windows."""
    # Non-Viper dependencies must be installed manually.
    # Download Viper.
    shell(
        'curl https://github.com/viperproject/viper-ide/releases/'
        'download/{}/ViperToolsWin.zip -Lo ViperToolsWin.zip'.format(viper_version())
    )
    if os.path.exists('viper_tools'):
        shutil.rmtree('viper_tools')
    os.mkdir('viper_tools')
    shell('tar -xf ViperToolsWin.zip -C viper_tools')
    os.remove('ViperToolsWin.zip')


def setup_rustup():
    # Update rustup
    shell('rustup self update', term_on_nzec=False)


def setup(args):
    """Install the dependencies."""
    rustup_only = False
    if len(args) == 1 and args[0] == '--dry-run':
        global dry_run
        dry_run = True
    elif len(args) == 1 and args[0] == '--rustup-only':
        rustup_only = True
    elif args:
        error("unexpected arguments: {}", args)
    if not rustup_only:
        if sys.platform in ("linux", "linux2"):
            if 'Ubuntu' in platform.platform():
                setup_ubuntu()
            else:
                setup_linux()
        elif sys.platform == "darwin":
            setup_mac()
        elif sys.platform == "win32":
            setup_win()
        else:
            error("unsupported platform: {}", sys.platform)
    setup_rustup()


def ide(args):
    """Start VS Code with the given arguments."""
    run_command(['code'] + args)

def run_benchmarks(args):
    """Run the benchmarks and report the time in a json file"""
    warmup_iterations = 6
    bench_iterations = 10
    warmup_path = "prusti-tests/tests/verify/pass/quick/fibonacci.rs"
    prusti_server_exe = get_prusti_server_path_for_benchmark()
    server_port = "12345"
    output_dir = "benchmark-output"
    benchmark_csv = "benchmarked-files.csv"
    results = {}
    
    report_name_suffix = ("-" + args[0]) if len(args) > 0 else ''

    env = get_env()
    report("Starting prusti-server ({})", prusti_server_exe)
    server_process = subprocess.Popen([prusti_server_exe,"--port",server_port], env=env)
    time.sleep(2)
    if server_process.poll() != None:
        raise RuntimeError('Could not start prusti-server') 

    env["PRUSTI_SERVER_ADDRESS"]="localhost:" + server_port
    try:
        report("Starting warmup of the server")
        for i in range(warmup_iterations):
            t = measure_prusti_time(warmup_path, env)
            report("warmup run {} took {}", i + 1, t)
        
        report("Finished warmup. Starting benchmark")
        with open(benchmark_csv) as csv_file:
            csv_reader = csv.reader(csv_file, delimiter=',')
            for row in csv_reader:
                file_path = row[0]
                results[file_path] = []
                report("Starting to benchmark {}", file_path)
                for i in range(bench_iterations):
                    t = measure_prusti_time(file_path, env)
                    results[file_path].append(t)
    finally:
        report("terminating prusti-server")
        server_process.send_signal(signal.SIGINT)

    if not os.path.exists(output_dir):
        os.makedirs(output_dir)

    json_result = json.dumps(results, indent = 2)
    timestamp = time.time()
    output_file = os.path.join(output_dir, "benchmark" + report_name_suffix + str(timestamp) + ".json")
    with open(output_file, "w") as outfile: 
        outfile.write(json_result) 
    
    report("Wrote results of benchmark to {}", output_file)


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


def measure_prusti_time(input_path, env):
    prusti_rustc_exe = get_prusti_rustc_path_for_benchmark()
    start_time = time.perf_counter()
    run_command([prusti_rustc_exe,"--edition=2018", input_path], env=env)
    end_time = time.perf_counter()
    elapsed = end_time - start_time
    return elapsed  



def select_newest_file(paths):
    """Select a file that exists and has the newest modification timestamp."""
    existing_paths = [
        (os.path.getmtime(path), path)
        for path in paths if os.path.exists(path)
    ]
    try:
        return next(reversed(sorted(existing_paths)))[1]
    except:
        error("Could not select the newest file from {}", paths)


def verify_test(args):
    """Runs prusti on the specified files."""
    test = None
    compile_flags = []
    for arg in args:
        if arg.startswith('-'):
            compile_flags.append(arg)
        else:
            if test is None:
                test = arg
            else:
                error("Expected a single argument (test file). Got: {}", args)

    current_path = os.path.abspath(os.path.curdir)
    candidate_prusti_paths = [
        os.path.join(current_path, 'target', 'release', 'prusti-rustc'),
        os.path.join(current_path, 'target', 'debug', 'prusti-rustc')
    ]
    prusti_path = select_newest_file(candidate_prusti_paths)
    report("Selected Prusti: {}", prusti_path)
    if test.startswith('prusti-tests/'):
        test_path = test
    else:
        candidate_test_paths = glob.glob(os.path.join(current_path, "prusti-tests/tests*/*", test))
        if len(candidate_test_paths) == 0:
            error("Not tests found that match: {}", test)
        elif len(candidate_test_paths) > 1:
            error(
                "Expected one test, but found {} tests that match {}. First 5: {}",
                len(candidate_test_paths),
                test,
                candidate_test_paths[:5]
            )
        test_path = candidate_test_paths[0]
    report("Found test: {}", test_path)
    with open(test_path) as fp:
        for line in fp:
            if line.startswith('// compile-flags:'):
                compile_flags.extend(line[len('// compile-flags:'):].strip().split())
        report("Additional compile flags: {}", compile_flags)
    env = get_env()
    if test_path.startswith('prusti-tests/tests/verify_overflow/'):
        env['PRUSTI_CHECK_OVERFLOWS'] = 'true'
    else:
        env['PRUSTI_CHECK_OVERFLOWS'] = 'false'
    run_command([prusti_path, '--edition=2018', test_path] + compile_flags, env)


def main(argv):
    global verbose
    for i, arg in enumerate(argv):
        if arg.startswith('+'):
            if arg == '+v' or arg == '++verbose':
                verbose = True
                continue
            else:
                error('unknown option: {}', arg)
        elif arg == 'setup':
            setup(argv[i+1:])
            break
        elif arg == 'ide':
            ide(argv[i+1:])
            break
        elif arg == 'run-benchmarks':
            run_benchmarks(argv[i+1:])
            break
        elif arg == 'verify-test':
            verify_test(argv[i+1:])
            break
        elif arg == 'exec':
            run_command(argv[i+1:])
            break
        else:
            cargo(argv[i:])
            break
    if not argv:
        cargo(argv)


if __name__ == '__main__':
    main(sys.argv[1:])