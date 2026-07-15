counter = 0
def increment():
    global counter
    counter += 1
increment()
increment()
print(counter)
def outer():
    x = 0
    def inner():
        nonlocal x
        x += 10
        return x
    inner()
    inner()
    return x
print(outer())
config = {"debug": False}
def enable_debug():
    global config
    config["debug"] = True
enable_debug()
print(config)
def make_counter():
    count = 0
    def counter_fn():
        nonlocal count
        count += 1
        return count
    return counter_fn
c1 = make_counter()
c2 = make_counter()
print(c1(), c1(), c2(), c1())
total = 100
def use_global():
    return total * 2
print(use_global())
