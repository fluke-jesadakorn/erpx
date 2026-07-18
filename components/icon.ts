import {
  ArrowDownUp,
  ArrowRight,
  Bell,
  BookOpen,
  Briefcase,
  Building2,
  Calendar,
  Check,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  CircleAlert,
  Copy,
  Cpu,
  Crown,
  Filter,
  Gauge,
  Globe,
  History,
  Home,
  Info,
  Inbox,
  Key,
  LayoutGrid,
  Lock,
  Mail,
  Menu,
  MessageCircle,
  Package,
  Paperclip,
  Receipt,
  Scale,
  Search,
  Send,
  Server,
  Settings,
  Shield,
  ShieldCheck,
  ShoppingCart,
  Star,
  Truck,
  User,
  UserCheck,
  Users,
  X,
  Zap,
  type LucideIcon,
} from 'lucide-react';

export type IconName =
  | 'ArrowDownUp'
  | 'ArrowRight'
  | 'Bell'
  | 'BookOpen'
  | 'Briefcase'
  | 'Building2'
  | 'Calendar'
  | 'Check'
  | 'CheckCircle'
  | 'ChevronDown'
  | 'ChevronRight'
  | 'ChevronUp'
  | 'CircleAlert'
  | 'Copy'
  | 'Cpu'
  | 'Crown'
  | 'Filter'
  | 'Gauge'
  | 'Globe'
  | 'History'
  | 'Home'
  | 'Info'
  | 'Inbox'
  | 'Key'
  | 'LayoutGrid'
  | 'Lock'
  | 'Mail'
  | 'Menu'
  | 'MessageCircle'
  | 'Package'
  | 'Paperclip'
  | 'Receipt'
  | 'Scale'
  | 'Search'
  | 'Send'
  | 'Server'
  | 'Settings'
  | 'Shield'
  | 'ShieldCheck'
  | 'ShoppingCart'
  | 'Star'
  | 'Truck'
  | 'User'
  | 'UserCheck'
  | 'Users'
  | 'X'
  | 'Zap';

const ICON_BY_NAME: Record<IconName, LucideIcon> = {
  ArrowDownUp,
  ArrowRight,
  Bell,
  BookOpen,
  Briefcase,
  Building2,
  Calendar,
  Check,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  CircleAlert,
  Copy,
  Cpu,
  Crown,
  Filter,
  Gauge,
  Globe,
  History,
  Home,
  Info,
  Inbox,
  Key,
  LayoutGrid,
  Lock,
  Mail,
  Menu,
  MessageCircle,
  Package,
  Paperclip,
  Receipt,
  Scale,
  Search,
  Send,
  Server,
  Settings,
  Shield,
  ShieldCheck,
  ShoppingCart,
  Star,
  Truck,
  User,
  UserCheck,
  Users,
  X,
  Zap,
};

export function iconByName(name: IconName): LucideIcon {
  return ICON_BY_NAME[name];
}
