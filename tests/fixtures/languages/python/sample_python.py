#!/usr/bin/env python3
"""
Sample Python code for testing chunking and indexing.
"""

import asyncio
from dataclasses import dataclass
from typing import List, Optional, Dict, Any
from datetime import datetime


@dataclass
class Task:
    """Represents an async task"""
    id: str
    name: str
    priority: int
    created_at: datetime
    completed: bool = False


class TaskQueue:
    """An async priority task queue"""

    def __init__(self, max_concurrent: int = 5):
        self.max_concurrent = max_concurrent
        self.tasks: List[Task] = []
        self.running: Dict[str, asyncio.Task] = {}
        self._lock = asyncio.Lock()

    async def add_task(self, task: Task) -> None:
        """Add a task to the queue"""
        async with self._lock:
            self.tasks.append(task)
            # Sort by priority (higher first)
            self.tasks.sort(key=lambda t: t.priority, reverse=True)

    async def process_tasks(self) -> None:
        """Process all tasks in the queue"""
        while self.tasks or self.running:
            async with self._lock:
                # Start new tasks up to max_concurrent
                while len(self.running) < self.max_concurrent and self.tasks:
                    task = self.tasks.pop(0)
                    coro = self._execute_task(task)
                    self.running[task.id] = asyncio.create_task(coro)

            if self.running:
                # Wait for at least one task to complete
                done, pending = await asyncio.wait(
                    self.running.values(),
                    return_when=asyncio.FIRST_COMPLETED
                )

                # Remove completed tasks
                async with self._lock:
                    completed_ids = [
                        task_id for task_id, task in self.running.items()
                        if task in done
                    ]
                    for task_id in completed_ids:
                        del self.running[task_id]

    async def _execute_task(self, task: Task) -> None:
        """Execute a single task"""
        print(f"Starting task: {task.name}")
        # Simulate work
        await asyncio.sleep(1.0 / task.priority)
        task.completed = True
        print(f"Completed task: {task.name}")


def decorator_with_args(prefix: str = "LOG"):
    """A decorator factory that adds logging"""
    def decorator(func):
        async def wrapper(*args, **kwargs):
            print(f"[{prefix}] Calling {func.__name__}")
            result = await func(*args, **kwargs)
            print(f"[{prefix}] Completed {func.__name__}")
            return result
        return wrapper
    return decorator


@decorator_with_args("WORKER")
async def worker_function(worker_id: int, data: List[Any]) -> List[Any]:
    """Process data asynchronously"""
    results = []
    for item in data:
        await asyncio.sleep(0.1)
        results.append(f"Worker {worker_id}: {item}")
    return results


async def main():
    """Main entry point"""
    # Create task queue
    queue = TaskQueue(max_concurrent=3)

    # Add tasks
    tasks = [
        Task(f"task_{i}", f"Task {i}", priority=i % 3 + 1, created_at=datetime.now())
        for i in range(10)
    ]

    for task in tasks:
        await queue.add_task(task)

    # Process all tasks
    await queue.process_tasks()

    # Run worker function
    data = list(range(5))
    results = await worker_function(1, data)
    print(f"Worker results: {results}")


if __name__ == "__main__":
    asyncio.run(main())