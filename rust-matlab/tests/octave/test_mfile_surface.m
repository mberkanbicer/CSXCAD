function test_mfile_surface()
% TEST_MFILE_SURFACE  RM-032: the MATLAB/Octave surface must match the native inventory.
%
% The native CSXCAD MATLAB API is delivered as the unchanged M-files in
% `CSXCAD/matlab` (58 public + 12 private, 114 function declarations), exactly
% like the native installation. `libcsxcad_matlab` (the Rust C ABI built from
% `CSXCAD/rust-matlab`) is an additional embedding path, not a replacement, so
% every native entry point has to resolve as an M-file.
%
% The expected surface is `tests/native_mfiles/native_mfile_inventory.json`,
% generated from the M-file sources by `native_mfile_inventory.py` (with sha256
% provenance). This script checks three things:
%   1. the inventory is current (each file still declares exactly what the JSON
%      records, and the file hashes match),
%   2. every public entry point resolves through `addpath(matlab)` to the right
%      file, and every private helper exists,
%   3. every entry point declares how it is delivered, and the `absent` list is
%      explicit (it is empty: nothing is missing).
%
% Run from the repository: octave --no-gui --quiet tests/octave/test_mfile_surface.m

  here = fileparts(mfilename('fullpath'));
  if exist('canonicalize_file_name', 'builtin') || exist('canonicalize_file_name', 'file')
    here = canonicalize_file_name(here);
  end
  crate = fullfile(here, '..', '..');             % CSXCAD/rust-matlab
  csxcad_root = fullfile(crate, '..');            % CSXCAD
  matlab_dir = fullfile(csxcad_root, 'matlab');
  inventory_path = fullfile(crate, 'tests', 'native_mfiles', 'native_mfile_inventory.json');

  assert_true(isfile(inventory_path), ['missing inventory: ' inventory_path]);
  inventory = jsondecode(fileread(inventory_path));

  counts = inventory.counts;
  assert_equal(counts.files, 70, 'M-file count');
  assert_equal(counts.public_files, 58, 'public M-file count');
  assert_equal(counts.private_files, 12, 'private M-file count');
  assert_equal(counts.declarations, 114, 'function declaration count');
  assert_equal(numel(inventory.public), 58, 'public entry point count');
  assert_equal(numel(inventory.private), 12, 'private entry point count');
  assert_equal(numel(inventory.sources), 70, 'provenance entry count');
  assert_equal(numel(inventory.declarations), 114, 'declaration record count');

  % 1. provenance and freshness ------------------------------------------
  for n = 1:numel(inventory.sources)
    source = inventory.sources(n);
    path = fullfile(csxcad_root, source.file);
    assert_true(isfile(path), ['missing M-file: ' source.file]);
    assert_true(numel(source.sha256) == 64, ['bad sha256 for ' source.file]);
    assert_equal(numel(fileread(path)), source.bytes, ...
                 ['file size changed, inventory is stale: ' source.file]);
    assert_equal(declaration_count(path), source.declarations, ...
                 ['declaration count changed, inventory is stale: ' source.file]);
  end

  % Every recorded declaration must still be what the file says.
  expected = struct();
  for n = 1:numel(inventory.declarations)
    declaration = inventory.declarations(n);
    key = declaration.file;
    if ~isfield(expected, key)
      expected.(key) = parse_declarations(fullfile(csxcad_root, key));
    end
  end
  for key = fieldnames(expected)'
    parsed = expected.(key{1});
    recorded = inventory.declarations(strcmp({inventory.declarations.file}, key{1}));
    assert_equal(numel(parsed), numel(recorded), ['declaration count for ' key{1}]);
    for n = 1:numel(parsed)
      assert_true(strcmp(parsed(n).name, recorded(n).name), ...
                  sprintf('%s:%d function name', key{1}, parsed(n).line));
      assert_true(isequal(parsed(n).args, as_names(recorded(n).args)), ...
                  sprintf('%s:%d argument list ("%s" vs "%s")', key{1}, parsed(n).line, ...
                          strjoin(parsed(n).args, ','), strjoin(as_names(recorded(n).args), ',')));
      assert_true(isequal(parsed(n).returns, as_names(recorded(n).returns)), ...
                  sprintf('%s:%d return values ("%s" vs "%s")', key{1}, parsed(n).line, ...
                          strjoin(parsed(n).returns, ','), strjoin(as_names(recorded(n).returns), ',')));
    end
  end

  % 2. resolution ---------------------------------------------------------
  addpath(matlab_dir);
  for n = 1:numel(inventory.public)
    entry = inventory.public(n);
    assert_true(exist(entry.name, 'file') == 2, ...
                ['public entry point does not resolve: ' entry.name]);
    resolved = which(entry.name);
    expected_path = fullfile(matlab_dir, [entry.name '.m']);
    assert_true(strcmp(normalise(resolved), normalise(expected_path)), ...
                sprintf('%s resolved to %s instead of %s', entry.name, resolved, expected_path));
    % every public M-file is delivered as the unchanged M-file
    assert_true(strcmp(entry.delivery, 'm-file'), ...
                [entry.name ' has an unexpected delivery method']);
  end

  for n = 1:numel(inventory.private)
    entry = inventory.private(n);
    path = fullfile(csxcad_root, entry.file);
    assert_true(isfile(path), ['missing private helper: ' entry.file]);
    assert_true(strcmp(entry.delivery, 'm-file'), ...
                [entry.name ' has an unexpected delivery method']);
  end

  % 3. explicit absent / requirement lists -------------------------------
  assert_true(isempty(fieldnames(inventory.absent)), 'the absent list must be empty');
  assert_true(isfield(inventory, 'absent_note'), 'the absent list needs a reason');
  assert_true(~isempty(inventory.absent_note), 'the absent list needs a reason');
  assert_true(isfield(inventory, 'delivery'), 'the inventory has no delivery record');
  assert_true(isfield(inventory.delivery, 'm_files'), 'missing M-file delivery record');
  assert_true(isfield(inventory.delivery, 'c_abi'), 'missing C ABI delivery record');
  assert_equal(numel(inventory.delivery.c_abi.symbols), 18, 'C ABI symbol count');
  for n = 1:numel(inventory.public)
    entry = inventory.public(n);
    assert_true(isfield(entry, 'delivery'), [entry.name ' has no delivery record']);
    assert_true(strcmp(entry.delivery, 'm-file'), ...
                [entry.name ' has an unexpected delivery method']);
    assert_true(isfield(entry, 'requires'), [entry.name ' has no requires record']);
    expected = {};
    if strcmp(entry.name, 'CSXGeomPlot')
      expected = {'binary:AppCSXCAD'};
    elseif strcmp(entry.name, 'CreateDiscMaterial')
      expected = {'octave-package:HDF5 (h5create)'};
    end
    assert_true(isequal(as_names(entry.requires), expected), ...
                sprintf('%s: external requirement record', entry.name));
  end

  printf('RM-032 M-file surface: %d public + %d private M-files, %d declarations, 0 absent\n', ...
         counts.public_files, counts.private_files, counts.declarations);
end

% ---------------------------------------------------------------------------
function count = declaration_count(path)
  count = 0;
  lines = strsplit(normalise_newlines(path), "\n");
  for n = 1:numel(lines)
    if ~isempty(regexp(lines{n}, '^\s*function\s', 'once'))
      count = count + 1;
    end
  end
end

function declarations = parse_declarations(path)
  declarations = struct('name', {}, 'args', {}, 'returns', {}, 'line', {});
  lines = strsplit(normalise_newlines(path), "\n");
  n = 1;
  while n <= numel(lines)
    line = lines{n};
    lineno = n;
    n = n + 1;
    if isempty(regexp(line, '^\s*function\s', 'once'))
      continue
    end
    % join MATLAB continuation lines so a wrapped signature is complete
    while numel(line) >= 3 && strcmp(line(end - 2:end), '...') && n <= numel(lines)
      line = [strtrim(line(1:end - 3)) ' ' strtrim(lines{n})];
      n = n + 1;
    end
    declaration = parse_declaration(line);
    if isempty(declaration)
      continue
    end
    declaration.line = lineno;
    declarations(end + 1) = declaration;
  end
end

function declaration = parse_declaration(line)
% Parse one MATLAB function declaration without relying on optional regexp
% groups (Octave drops non-participating groups from the token list):
%   function name(args)
%   function ret = name(args)
%   function [a, b] = name(args)
%   function [a b] = name(args)
  declaration = [];
  if isempty(regexp(line, '^\s*function\s', 'once'))
    return
  end
  body = strtrim(regexprep(line, '^\s*function\s+', ''));
  returns = {};
  open = strfind(body, '(');
  if isempty(open)
    open = numel(body) + 1;
  end
  equals = strfind(body, '=');
  equals = equals(equals < open);
  if ~isempty(equals)
    lhs = strtrim(body(1:equals(1) - 1));
    body = strtrim(body(equals(1) + 1:end));
    if numel(lhs) >= 2 && lhs(1) == '[' && lhs(end) == ']'
      returns = split_names(lhs(2:end - 1));
    else
      returns = {lhs};
    end
    open = strfind(body, '(');
    if isempty(open)
      open = numel(body) + 1;
    end
  end
  if isempty(open) || open > numel(body)
    name = strtrim(body);
    args = {};
  else
    name = strtrim(body(1:open - 1));
    % the closing parenthesis is not necessarily the last character
    % (`struct_2_xml.m` ends the declaration with a `;`)
    close_at = strfind(body, ')');
    if isempty(close_at)
      args = split_names(body(open + 1:end));
    else
      args = split_names(body(open + 1:close_at(1) - 1));
    end
  end
  if isempty(name)
    return
  end
  declaration = struct('name', name, 'args', {args}, 'returns', {returns}, ...
                       'line', 0);
end

function text = normalise_newlines(path)
% The native M-files are checked out with CRLF on Windows; the parser works on
% LF-only text so a trailing \r cannot hide a continuation marker.
  text = strrep(fileread(path), "\r\n", "\n");
  text = strrep(text, "\r", "\n");
end

function names = split_names(text)
  if isempty(text)
    names = {};
    return
  end
  parts = regexp(strtrim(text), '[,\s]+', 'split');
  names = parts(~cellfun('isempty', parts));
end

function names = as_names(value)
% Normalise a jsondecode'd name list to a row cell array of strings.
  if ischar(value)
    names = {value};
  elseif iscell(value)
    names = reshape(value, 1, []);
    names = names(~cellfun('isempty', names));
  elseif isempty(value)
    names = {};
  else
    names = {num2str(value)};
  end
end

function path = normalise(path)
  path = strrep(path, '\', '/');
  if exist('canonicalize_file_name', 'builtin') || exist('canonicalize_file_name', 'file')
    path = canonicalize_file_name(path);
  end
end

function assert_true(condition, message)
  if ~condition
    error('RM032:assert', '%s', message);
  end
end

function assert_equal(actual, expected, message)
  if ~isequal(actual, expected)
    error('RM032:assert', '%s: expected %s, got %s', message, ...
          mat2str(expected), mat2str(actual));
  end
end
