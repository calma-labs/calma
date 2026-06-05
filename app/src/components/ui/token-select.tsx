import * as Select from "@radix-ui/react-select"
import { cn } from "@/lib/utils"
import { ChevronDown, Check } from "lucide-react"

export interface TokenOption {
    address: string
    symbol: string
    icon: string
}

interface TokenSelectProps {
    value: string
    onChange: (address: string) => void
    options: TokenOption[]
    placeholder?: string
}

export function TokenSelect({ value, onChange, options, placeholder }: TokenSelectProps) {
    const selected = options.find((o) => o.address === value)

    return (
        <Select.Root value={value} onValueChange={onChange}>
            <Select.Trigger
                className={cn(
                    "flex w-full items-center gap-2.5 rounded-xl border border-[#c698e5]/20 bg-[#c698e5]/[0.04]",
                    "px-4 py-2.5 text-sm text-[#efe0f7] outline-none transition-colors duration-150 cursor-pointer",
                    "hover:border-[#c698e5]/35 focus:border-[#c698e5]/50 focus:bg-[#c698e5]/[0.07]",
                    "data-[state=open]:border-[#c698e5]/50 data-[state=open]:bg-[#c698e5]/[0.07]",
                )}
            >
                {selected && (
                    <img src={selected.icon} alt="" className="h-5 w-5 rounded-full shrink-0 object-contain" />
                )}
                <Select.Value
                    placeholder={<span className="text-[#efe0f7]/25">{placeholder ?? "Select token"}</span>}
                    className="flex-1 text-left"
                />
                <Select.Icon>
                    <ChevronDown className="h-3.5 w-3.5 text-[#c698e5]/50" />
                </Select.Icon>
            </Select.Trigger>

            <Select.Portal>
                <Select.Content
                    position="popper"
                    sideOffset={4}
                    className={cn(
                        "z-50 w-[var(--radix-select-trigger-width)] overflow-hidden",
                        "rounded-xl border border-[#c698e5]/15 bg-[#1f0e2b] p-1 shadow-2xl",
                        "animate-in fade-in-0 zoom-in-95",
                    )}
                >
                    <Select.Viewport>
                        {options.map((opt) => (
                            <Select.Item
                                key={opt.address}
                                value={opt.address}
                                className={cn(
                                    "flex items-center gap-2.5 rounded-lg px-3 py-2 text-sm outline-none cursor-pointer select-none",
                                    "text-[#efe0f7]/70 transition-colors",
                                    "data-[highlighted]:bg-[#c698e5]/10 data-[highlighted]:text-[#efe0f7]",
                                    "data-[state=checked]:text-[#c698e5]",
                                )}
                            >
                                <img src={opt.icon} alt="" className="h-5 w-5 rounded-full shrink-0 object-contain" />
                                <Select.ItemText>{opt.symbol}</Select.ItemText>
                                <Select.ItemIndicator className="ml-auto">
                                    <Check className="h-3.5 w-3.5" />
                                </Select.ItemIndicator>
                            </Select.Item>
                        ))}
                    </Select.Viewport>
                </Select.Content>
            </Select.Portal>
        </Select.Root>
    )
}
