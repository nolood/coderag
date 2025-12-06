/**
 * Sample TypeScript code for testing
 */

interface User {
    id: string;
    name: string;
    email: string;
    age: number;
    roles: Role[];
}

interface Role {
    id: string;
    name: string;
    permissions: Permission[];
}

type Permission = 'read' | 'write' | 'delete' | 'admin';

class UserService {
    private users: Map<string, User>;
    private cache: Map<string, any>;

    constructor() {
        this.users = new Map();
        this.cache = new Map();
    }

    /**
     * Add a new user to the service
     */
    async addUser(user: User): Promise<void> {
        if (this.users.has(user.id)) {
            throw new Error(`User ${user.id} already exists`);
        }
        this.users.set(user.id, user);
        this.invalidateCache();
    }

    /**
     * Get a user by ID with caching
     */
    async getUser(id: string): Promise<User | undefined> {
        const cacheKey = `user_${id}`;

        if (this.cache.has(cacheKey)) {
            return this.cache.get(cacheKey);
        }

        const user = this.users.get(id);
        if (user) {
            this.cache.set(cacheKey, user);
        }

        return user;
    }

    /**
     * Find users by role
     */
    async findUsersByRole(roleName: string): Promise<User[]> {
        const cacheKey = `role_${roleName}`;

        if (this.cache.has(cacheKey)) {
            return this.cache.get(cacheKey);
        }

        const users = Array.from(this.users.values()).filter(user =>
            user.roles.some(role => role.name === roleName)
        );

        this.cache.set(cacheKey, users);
        return users;
    }

    /**
     * Check if user has permission
     */
    hasPermission(user: User, permission: Permission): boolean {
        return user.roles.some(role =>
            role.permissions.includes(permission)
        );
    }

    private invalidateCache(): void {
        this.cache.clear();
    }
}

// Generic utility functions
function debounce<T extends (...args: any[]) => any>(
    func: T,
    wait: number
): (...args: Parameters<T>) => void {
    let timeout: NodeJS.Timeout | null = null;

    return function(this: any, ...args: Parameters<T>) {
        const context = this;

        if (timeout) {
            clearTimeout(timeout);
        }

        timeout = setTimeout(() => {
            func.apply(context, args);
        }, wait);
    };
}

async function retry<T>(
    fn: () => Promise<T>,
    maxAttempts: number = 3,
    delay: number = 1000
): Promise<T> {
    let lastError: Error | undefined;

    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
        try {
            return await fn();
        } catch (error) {
            lastError = error as Error;
            if (attempt < maxAttempts) {
                await new Promise(resolve => setTimeout(resolve, delay * attempt));
            }
        }
    }

    throw lastError;
}

// React-like component (for TSX testing)
export const UserList: React.FC<{ users: User[] }> = ({ users }) => {
    const [filter, setFilter] = React.useState('');
    const [sortBy, setSortBy] = React.useState<'name' | 'age'>('name');

    const filteredUsers = React.useMemo(() => {
        return users
            .filter(user =>
                user.name.toLowerCase().includes(filter.toLowerCase())
            )
            .sort((a, b) => {
                if (sortBy === 'name') {
                    return a.name.localeCompare(b.name);
                }
                return a.age - b.age;
            });
    }, [users, filter, sortBy]);

    return (
        <div>
            <input
                type="text"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Filter users..."
            />
            <select value={sortBy} onChange={(e) => setSortBy(e.target.value as 'name' | 'age')}>
                <option value="name">Sort by Name</option>
                <option value="age">Sort by Age</option>
            </select>
            <ul>
                {filteredUsers.map(user => (
                    <li key={user.id}>
                        {user.name} ({user.age})
                    </li>
                ))}
            </ul>
        </div>
    );
};

export { UserService, debounce, retry };