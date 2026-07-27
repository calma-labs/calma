import { useEffect, useState } from 'react'

/** Current Unix timestamp in seconds, updated every `intervalMs` milliseconds. */
export function useNow(intervalMs = 1_000): number {
    const [now, setNow] = useState(() => Math.floor(Date.now() / 1000))
    useEffect(() => {
        const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), intervalMs)
        return () => clearInterval(id)
    }, [intervalMs])
    return now
}
