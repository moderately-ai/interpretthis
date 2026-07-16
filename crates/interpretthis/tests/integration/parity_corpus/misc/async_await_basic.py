import asyncio


async def main():
    return 42


print(asyncio.run(main()))


async def add(a, b):
    return a + b


async def compute():
    x = await add(2, 3)
    y = await add(10, 20)
    return x + y


print(asyncio.run(compute()))


async def greet(name):
    return f"Hello, {name}"


async def run_all():
    return await asyncio.gather(greet("a"), greet("b"), greet("c"))


print(asyncio.run(run_all()))


async def with_sleep():
    await asyncio.sleep(0)
    return "slept"


print(asyncio.run(with_sleep()))


async def nested():
    async def inner(x):
        return x * 2

    a = await inner(5)
    return await inner(a)


print(asyncio.run(nested()))


async def uses_task():
    t = asyncio.create_task(add(1, 2))
    return await t


print(asyncio.run(uses_task()))


async def loop_await():
    total = 0
    for i in range(5):
        total += await add(i, i)
    return total


print(asyncio.run(loop_await()))


async def gather_sum():
    vals = await asyncio.gather(*[add(i, i) for i in range(4)])
    return sum(vals)


print(asyncio.run(gather_sum()))


async def with_exception():
    try:
        await add(1, 2)
        raise ValueError("boom")
    except ValueError as e:
        return f"caught {e}"


print(asyncio.run(with_exception()))

print(type(main()).__name__)
