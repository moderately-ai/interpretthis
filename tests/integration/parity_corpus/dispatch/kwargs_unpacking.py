# Pins: **kwargs unpacking in calls; *args/**kwargs forwarding;
# nested generator expression as sum() argument. Heavy customer
# pattern for option-bag helpers.
def greet(name, greeting="Hello", punct="!"):
    return f"{greeting}, {name}{punct}"

opts = {"greeting": "Hi", "punct": "?"}
print(greet("Alice", **opts))

def wrap(f, *args, **kwargs):
    return f(*args, **kwargs)

print(wrap(greet, "Bob", greeting="Hey"))

print(sum(i * i for i in range(5)))

print({k: v for k, v in [("a", 1), ("b", 2), ("c", 3)] if v % 2 == 1})
