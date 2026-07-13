# Pins: f-string `=` debug specifier renders the source expression
# alongside its value. Heavy in agent-emitted print-debugging.
x = 42
y = 3.14
print(f"{x=}")
print(f"{y=:.2f}")
print(f"{x + 1 = }")
