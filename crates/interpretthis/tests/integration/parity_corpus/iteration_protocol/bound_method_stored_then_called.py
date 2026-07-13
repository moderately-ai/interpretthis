# Pins: `fn = d.get; fn('A')` — store bound method then call. Customer-listed
# pattern. Routes through eval_call's variable-lookup branch, which
# historically had no BoundMethod arm and would error "'fn' is not callable".
d = {'A': 1, 'B': 2}
fn = d.get
print(fn('A'))
