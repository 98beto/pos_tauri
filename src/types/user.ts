export interface User {
  id: number;
  first_name: string;
  last_name: string;
  email: string;
  created_at: string;
  updated_at: string;
}

export interface LoginInput {
  email: string;
  password: string;
}

export interface InitializeOwnerInput extends LoginInput {
  first_name: string;
  last_name: string;
  confirm_password: string;
}
