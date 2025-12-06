package main

import (
    "context"
    "fmt"
    "sync"
    "time"
)

// Task represents a unit of work
type Task struct {
    ID       int
    Name     string
    Duration time.Duration
    Result   interface{}
}

// WorkerPool manages a pool of workers
type WorkerPool struct {
    workers   int
    taskQueue chan Task
    results   chan Task
    wg        sync.WaitGroup
    ctx       context.Context
    cancel    context.CancelFunc
}

// NewWorkerPool creates a new worker pool
func NewWorkerPool(workers int, queueSize int) *WorkerPool {
    ctx, cancel := context.WithCancel(context.Background())
    return &WorkerPool{
        workers:   workers,
        taskQueue: make(chan Task, queueSize),
        results:   make(chan Task, queueSize),
        ctx:       ctx,
        cancel:    cancel,
    }
}

// Start begins processing tasks
func (wp *WorkerPool) Start() {
    for i := 0; i < wp.workers; i++ {
        wp.wg.Add(1)
        go wp.worker(i)
    }
}

// worker processes tasks from the queue
func (wp *WorkerPool) worker(id int) {
    defer wp.wg.Done()

    for {
        select {
        case task, ok := <-wp.taskQueue:
            if !ok {
                fmt.Printf("Worker %d: queue closed\n", id)
                return
            }

            fmt.Printf("Worker %d: processing task %d\n", id, task.ID)

            // Simulate work
            time.Sleep(task.Duration)
            task.Result = fmt.Sprintf("Completed by worker %d", id)

            select {
            case wp.results <- task:
            case <-wp.ctx.Done():
                return
            }

        case <-wp.ctx.Done():
            fmt.Printf("Worker %d: context cancelled\n", id)
            return
        }
    }
}

// Submit adds a task to the queue
func (wp *WorkerPool) Submit(task Task) error {
    select {
    case wp.taskQueue <- task:
        return nil
    case <-wp.ctx.Done():
        return fmt.Errorf("worker pool is shutting down")
    }
}

// GetResult retrieves a completed task
func (wp *WorkerPool) GetResult() (Task, bool) {
    select {
    case result := <-wp.results:
        return result, true
    default:
        return Task{}, false
    }
}

// Shutdown gracefully stops the worker pool
func (wp *WorkerPool) Shutdown() {
    close(wp.taskQueue)
    wp.wg.Wait()
    wp.cancel()
    close(wp.results)
}

// Generic constraint example (Go 1.18+)
type Number interface {
    ~int | ~int32 | ~int64 | ~float32 | ~float64
}

// Sum calculates the sum of a slice of numbers
func Sum[T Number](numbers []T) T {
    var sum T
    for _, n := range numbers {
        sum += n
    }
    return sum
}

// Pipeline pattern implementation
func Pipeline(input <-chan int) <-chan int {
    // Stage 1: Double
    doubled := make(chan int)
    go func() {
        defer close(doubled)
        for n := range input {
            doubled <- n * 2
        }
    }()

    // Stage 2: Add 10
    added := make(chan int)
    go func() {
        defer close(added)
        for n := range doubled {
            added <- n + 10
        }
    }()

    // Stage 3: Square
    squared := make(chan int)
    go func() {
        defer close(squared)
        for n := range added {
            squared <- n * n
        }
    }()

    return squared
}

func main() {
    // Create worker pool
    pool := NewWorkerPool(3, 10)
    pool.Start()

    // Submit tasks
    for i := 0; i < 10; i++ {
        task := Task{
            ID:       i,
            Name:     fmt.Sprintf("Task %d", i),
            Duration: time.Millisecond * time.Duration(100*(i%3+1)),
        }
        if err := pool.Submit(task); err != nil {
            fmt.Printf("Failed to submit task: %v\n", err)
        }
    }

    // Collect results
    time.Sleep(time.Second * 2)
    for i := 0; i < 10; i++ {
        if result, ok := pool.GetResult(); ok {
            fmt.Printf("Result: Task %d - %v\n", result.ID, result.Result)
        }
    }

    // Shutdown pool
    pool.Shutdown()

    // Test generic function
    numbers := []int{1, 2, 3, 4, 5}
    total := Sum(numbers)
    fmt.Printf("Sum: %d\n", total)

    // Test pipeline
    input := make(chan int)
    output := Pipeline(input)

    go func() {
        for i := 1; i <= 5; i++ {
            input <- i
        }
        close(input)
    }()

    for result := range output {
        fmt.Printf("Pipeline result: %d\n", result)
    }
}