def repeat(n):
    def decorator(func):
        def wrapper(*args, **kwargs):
            result = None
            for _ in range(n):
                result = func(*args, **kwargs)
            return result
        return wrapper
    return decorator
@repeat(3)
def greet(name):
    print(f"hi {name}")
    return name
print(greet("bob"))
def logged(func):
    def wrapper(*args):
        print(f"calling {func.__name__}")
        return func(*args)
    return wrapper
@logged
def add(a, b):
    return a + b
print(add(2, 3))
