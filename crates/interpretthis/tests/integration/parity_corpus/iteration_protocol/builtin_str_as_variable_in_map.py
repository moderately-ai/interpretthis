# Pins: aliased builtin passed to map via the variable. Combines the
# eval_call variable-lookup + call_value_as_function paths.
to_str = str
print(list(map(to_str, [1, 2, 3])))
