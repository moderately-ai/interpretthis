# Pins: storing a list's bound `append` and calling it multiple times.
# Common idiom in code-generation that builds an accumulator.
# Our value-semantics divergence from CPython noted on Value::BoundMethod:
# mutations don't propagate back to the original receiver. This snippet
# pins the customer-visible behavior — the appends must be reflected in
# the original list. If our snapshot semantics make this print [], that's
# a divergence worth pinning so we either fix or document it.
xs = []
push = xs.append
push(1); push(2); push(3)
print(xs)
