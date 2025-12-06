package com.example.coderag;

import java.util.*;
import java.util.concurrent.*;
import java.util.stream.Collectors;
import java.util.function.Function;
import java.util.function.Predicate;

/**
 * Sample Java code for testing chunking
 */
public class DataProcessor {

    private final ExecutorService executor;
    private final Map<String, CachedData> cache;
    private final int maxCacheSize;

    public DataProcessor(int threadPoolSize, int maxCacheSize) {
        this.executor = Executors.newFixedThreadPool(threadPoolSize);
        this.cache = new ConcurrentHashMap<>();
        this.maxCacheSize = maxCacheSize;
    }

    /**
     * Process data items in parallel
     */
    public <T, R> CompletableFuture<List<R>> processInParallel(
            List<T> items,
            Function<T, R> processor) {

        List<CompletableFuture<R>> futures = items.stream()
                .map(item -> CompletableFuture.supplyAsync(() -> processor.apply(item), executor))
                .collect(Collectors.toList());

        return CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]))
                .thenApply(v -> futures.stream()
                        .map(CompletableFuture::join)
                        .collect(Collectors.toList()));
    }

    /**
     * Generic repository pattern
     */
    public static class Repository<T extends Entity> {
        private final Map<Long, T> storage = new ConcurrentHashMap<>();
        private final AtomicLong idGenerator = new AtomicLong(0);

        public T save(T entity) {
            if (entity.getId() == null) {
                entity.setId(idGenerator.incrementAndGet());
            }
            storage.put(entity.getId(), entity);
            return entity;
        }

        public Optional<T> findById(Long id) {
            return Optional.ofNullable(storage.get(id));
        }

        public List<T> findAll() {
            return new ArrayList<>(storage.values());
        }

        public List<T> findByPredicate(Predicate<T> predicate) {
            return storage.values().stream()
                    .filter(predicate)
                    .collect(Collectors.toList());
        }

        public void delete(Long id) {
            storage.remove(id);
        }
    }

    /**
     * Base entity class
     */
    public static abstract class Entity {
        private Long id;
        private Date createdAt;
        private Date updatedAt;

        public Long getId() {
            return id;
        }

        public void setId(Long id) {
            this.id = id;
        }

        public Date getCreatedAt() {
            return createdAt;
        }

        public void setCreatedAt(Date createdAt) {
            this.createdAt = createdAt;
        }

        public Date getUpdatedAt() {
            return updatedAt;
        }

        public void setUpdatedAt(Date updatedAt) {
            this.updatedAt = updatedAt;
        }
    }

    /**
     * User entity
     */
    public static class User extends Entity {
        private String username;
        private String email;
        private Set<String> roles;

        public User(String username, String email) {
            this.username = username;
            this.email = email;
            this.roles = new HashSet<>();
            this.setCreatedAt(new Date());
            this.setUpdatedAt(new Date());
        }

        // Getters and setters
        public String getUsername() { return username; }
        public void setUsername(String username) { this.username = username; }
        public String getEmail() { return email; }
        public void setEmail(String email) { this.email = email; }
        public Set<String> getRoles() { return roles; }
        public void setRoles(Set<String> roles) { this.roles = roles; }
    }

    /**
     * Cached data wrapper
     */
    private static class CachedData {
        private final Object data;
        private final long timestamp;

        public CachedData(Object data) {
            this.data = data;
            this.timestamp = System.currentTimeMillis();
        }

        public boolean isExpired(long maxAgeMillis) {
            return System.currentTimeMillis() - timestamp > maxAgeMillis;
        }

        public Object getData() {
            return data;
        }
    }

    /**
     * Cache management
     */
    public void putInCache(String key, Object data) {
        if (cache.size() >= maxCacheSize) {
            // Simple LRU: remove oldest entry
            cache.entrySet().stream()
                    .min(Comparator.comparing(e -> e.getValue().timestamp))
                    .ifPresent(e -> cache.remove(e.getKey()));
        }
        cache.put(key, new CachedData(data));
    }

    public Optional<Object> getFromCache(String key, long maxAgeMillis) {
        CachedData cached = cache.get(key);
        if (cached != null && !cached.isExpired(maxAgeMillis)) {
            return Optional.of(cached.getData());
        }
        cache.remove(key);
        return Optional.empty();
    }

    /**
     * Builder pattern example
     */
    public static class QueryBuilder {
        private String table;
        private List<String> selectColumns = new ArrayList<>();
        private List<String> whereConditions = new ArrayList<>();
        private String orderBy;
        private Integer limit;

        public QueryBuilder from(String table) {
            this.table = table;
            return this;
        }

        public QueryBuilder select(String... columns) {
            this.selectColumns.addAll(Arrays.asList(columns));
            return this;
        }

        public QueryBuilder where(String condition) {
            this.whereConditions.add(condition);
            return this;
        }

        public QueryBuilder orderBy(String column) {
            this.orderBy = column;
            return this;
        }

        public QueryBuilder limit(int limit) {
            this.limit = limit;
            return this;
        }

        public String build() {
            StringBuilder query = new StringBuilder("SELECT ");

            if (selectColumns.isEmpty()) {
                query.append("*");
            } else {
                query.append(String.join(", ", selectColumns));
            }

            query.append(" FROM ").append(table);

            if (!whereConditions.isEmpty()) {
                query.append(" WHERE ").append(String.join(" AND ", whereConditions));
            }

            if (orderBy != null) {
                query.append(" ORDER BY ").append(orderBy);
            }

            if (limit != null) {
                query.append(" LIMIT ").append(limit);
            }

            return query.toString();
        }
    }

    /**
     * Shutdown the processor
     */
    public void shutdown() {
        executor.shutdown();
        try {
            if (!executor.awaitTermination(60, TimeUnit.SECONDS)) {
                executor.shutdownNow();
            }
        } catch (InterruptedException e) {
            executor.shutdownNow();
            Thread.currentThread().interrupt();
        }
    }

    public static void main(String[] args) {
        DataProcessor processor = new DataProcessor(4, 100);

        // Test repository
        Repository<User> userRepo = new Repository<>();
        User user1 = new User("john_doe", "john@example.com");
        user1.getRoles().add("USER");
        userRepo.save(user1);

        // Test query builder
        String query = new QueryBuilder()
                .from("users")
                .select("id", "username", "email")
                .where("active = true")
                .orderBy("created_at DESC")
                .limit(10)
                .build();

        System.out.println("Query: " + query);

        // Test parallel processing
        List<Integer> numbers = Arrays.asList(1, 2, 3, 4, 5);
        CompletableFuture<List<Integer>> future = processor.processInParallel(
                numbers,
                n -> n * n
        );

        future.thenAccept(results -> {
            System.out.println("Squared numbers: " + results);
        });

        processor.shutdown();
    }
}